//! ServWare MCP server (stdio), shipped inside the `.mcpb` extension.
//!
//! Credentials arrive as environment variables injected by Claude Desktop from
//! its encrypted user config; the model never sees them. See DECISIONS.md D4.
//!
//! Read-only tools carry `read_only_hint` so the client can auto-approve them.
//! Six approval dialogs before anything happens loses this audience, so friction
//! is reserved for writes.

use std::sync::Arc;

use rmcp::ErrorData;
use rmcp::ServerHandler;
use rmcp::ServiceExt;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::model::ContentBlock;
use rmcp::model::ServerCapabilities;
use rmcp::model::PromptMessage;
use rmcp::model::ServerInfo;
use rmcp::prompt;
use rmcp::prompt_handler;
use rmcp::prompt_router;
use rmcp::tool;
use rmcp::tool_handler;
use rmcp::tool_router;
use rmcp::transport::stdio;
use schemars::JsonSchema;
use serde::Deserialize;

use svdp::domain::policy::ConferenceConfig;
use svdp::domain::session::Delivery;
use svdp::domain::session::DeliveryOutcome;
use svdp::domain::session::DeliverySession;
use svdp::domain::session::Group;
use svdp::domain::session::SessionState;
use svdp::domain::session::SlotState;
use svdp::domain::store::SessionStore;
use svdp::domain::submit::submit;
use svdp::servware::write::ServWareBackend;
use svdp::servware::client::Credentials;
use svdp::servware::client::PUBLIC_BASE_URL;
use svdp::servware::client::ServWareClient;
use svdp::servware::detail;
use svdp::servware::error::ServWareError;
use svdp::servware::list;
use svdp::servware::list::RequestSummary;
use svdp::servware::list::StatusFilter;

#[derive(Clone)]
struct Svdp {
    client: Arc<ServWareClient>,
    config: Arc<ConferenceConfig>,
    store: Arc<SessionStore>,
}

/// One family in a delivery plan, as the model supplies it.
#[derive(Debug, Deserialize, JsonSchema)]
struct PlannedDelivery {
    /// Request id from `list_open_requests`.
    request_id: u64,
    /// Dollars of grocery gift card actually handed over. The volunteer's figure
    /// always wins over the amount household size suggests.
    gift_card_dollars: u32,
    /// Dollars of food. Omit to use the conference's standard amount.
    food_dollars: Option<u32>,
}

/// One volunteer group and the families it delivered to.
#[derive(Debug, Deserialize, JsonSchema)]
struct PlannedGroup {
    /// Volunteer id from `list_volunteers`.
    volunteer_id: String,
    /// Volunteer name, echoed back to the user for confirmation.
    volunteer_name: String,
    deliveries: Vec<PlannedDelivery>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct PlanParams {
    /// Every group and delivery for the day. This REPLACES any existing plan,
    /// so always send the complete picture, not just what changed.
    groups: Vec<PlannedGroup>,
    /// Delivery date as MM/DD/YYYY. Omit for today.
    delivery_date: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct ConfirmParams {
    /// The session id from `update_session_plan`.
    session_id: String,
    /// The revision shown to the user. Refuses if the plan has changed since.
    revision: u32,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct SessionIdParams {
    session_id: String,
}

/// One open request, projected down to what a delivery conversation needs.
///
/// Deliberately *not* the full ServWare record: `api.md` warns the conference
/// and district blocks are duplicated into every row, and pouring that into the
/// context window pushes the model into summarising -- i.e. inventing -- dollar
/// rows. See DECISIONS.md D6.
#[derive(Debug)]
struct OpenRequestView {
    request_id: u64,
    name: String,
    household_size: u32,
    address: String,
    phone: String,
    date_requested: String,
    days_open: i64,
    suggested_gift_card_dollars: u32,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct RequestIdParams {
    /// The ServWare request id, as shown by `list_open_requests`.
    request_id: u64,
}

#[tool_router]
impl Svdp {
    /// Checks that ServWare is reachable, the sign-in works, and the request and
    /// assistance-type formats are still what this tool expects. Run this first.
    #[tool(name = "servware_health", annotations(read_only_hint = true))]
    async fn servware_health(&self) -> Result<CallToolResult, ErrorData> {
        let mut lines = Vec::new();

        if let Err(e) = self.client.login().await {
            return Ok(fail(&e));
        }
        lines.push("Signed in to ServWare.".to_string());

        // Reading the list is also the schema check: required fields that went
        // missing surface here as a parse failure rather than as wrong money.
        match list::fetch_all(&self.client, StatusFilter::Open).await {
            Ok(requests) => {
                lines.push(format!("Read {} open requests.", requests.len()));
                if let Some(first) = requests.first() {
                    match detail::fetch(&self.client, first.id).await {
                        Ok(d) => {
                            lines.push(format!(
                                "Read a request page: {} volunteers, {} assistance items.",
                                d.members.len(),
                                d.assistance_items.len()
                            ));
                            for marker in ["status", "visitCompleted", "visitAssignedToMemberId"] {
                                if !d.form.contains(marker) {
                                    lines.push(format!(
                                        "PROBLEM: ServWare's form no longer has `{marker}`. \
                                         This tool needs an update before it writes anything."
                                    ));
                                }
                            }
                        }
                        Err(e) => return Ok(fail(&e)),
                    }
                } else {
                    lines.push("No open requests right now.".into());
                }
            }
            Err(e) => return Ok(fail(&e)),
        }

        lines.push(format!(
            "Gift cards ${}-${} by household size; food logged at ${}.",
            self.config.gift_card_dollars(1),
            self.config.gift_card_dollars(99),
            self.config.second_harvest.value.unwrap_or(0),
        ));
        Ok(ok(lines.join("\n")))
    }

    /// Lists the open assistance requests, oldest first, with the gift card
    /// amount each household size calls for. Use before recording a delivery.
    #[tool(name = "list_open_requests", annotations(read_only_hint = true))]
    async fn list_open_requests(&self) -> Result<CallToolResult, ErrorData> {
        let requests = match list::fetch_all(&self.client, StatusFilter::Open).await {
            Ok(r) => r,
            Err(e) => return Ok(fail(&e)),
        };
        let today = chrono::Local::now().date_naive();
        let mut views: Vec<OpenRequestView> = requests
            .iter()
            .map(|r| self.project(r, today))
            .collect();
        views.sort_by_key(|v| -v.days_open);

        Ok(ok(render_table(&views)))
    }

    /// Lists the volunteers a delivery can be credited to, with their ServWare
    /// ids. Names come from the request page's assignment dropdown.
    #[tool(name = "list_volunteers", annotations(read_only_hint = true))]
    async fn list_volunteers(&self) -> Result<CallToolResult, ErrorData> {
        let requests = match list::fetch_all(&self.client, StatusFilter::Open).await {
            Ok(r) => r,
            Err(e) => return Ok(fail(&e)),
        };
        let Some(first) = requests.first() else {
            return Ok(ok("There are no open requests, so the volunteer list \
                          cannot be read right now."));
        };
        match detail::fetch(&self.client, first.id).await {
            Ok(d) => {
                let lines: Vec<String> = d
                    .members
                    .iter()
                    .map(|m| format!("{} (id {})", m.name, m.id))
                    .collect();
                Ok(ok(format!("{} volunteers:\n{}", lines.len(), lines.join("\n"))))
            }
            Err(e) => Ok(fail(&e)),
        }
    }

    /// Shows what is already recorded against one request, including any food or
    /// gift card items already logged. Use to check before writing again.
    #[tool(name = "get_request", annotations(read_only_hint = true))]
    async fn get_request(
        &self,
        Parameters(p): Parameters<RequestIdParams>,
    ) -> Result<CallToolResult, ErrorData> {
        match detail::fetch(&self.client, p.request_id).await {
            Ok(d) => {
                let mut lines = vec![format!("Request {} is {}.", d.id, d.status())];
                if d.assistance_items.is_empty() {
                    lines.push("Nothing has been logged against it yet.".into());
                } else {
                    lines.push("Already logged:".into());
                    for item in &d.assistance_items {
                        lines.push(format!(
                            "  {} ${} on {}{}",
                            item.kind,
                            item.value,
                            item.date_provided,
                            if item.tag.is_some() { " (recorded by this tool)" } else { "" }
                        ));
                    }
                }
                Ok(ok(lines.join("\n")))
            }
            Err(e) => Ok(fail(&e)),
        }
    }

    /// Records what was delivered today, to whom, and by which volunteer.
    /// Send the COMPLETE plan every time; this replaces any previous version.
    /// Nothing is written to ServWare until confirm_plan and submit_session.
    #[tool(name = "update_session_plan")]
    async fn update_session_plan(
        &self,
        Parameters(p): Parameters<PlanParams>,
    ) -> Result<CallToolResult, ErrorData> {
        // Refuse to re-plan a delivery that has already sent money: the audit
        // trail of what reached ServWare must not be overwritten.
        if let Ok(Some(existing)) = self.store.current()
            && existing.has_written() {
                return Ok(ok(format!(
                    "A delivery recorded on {} is still part-way through being saved. \n                     Check on it with get_session (id {}) and finish or abandon it \n                     before starting another.",
                    existing.delivery_date, existing.id
                )));
            }

        let open = match list::fetch_all(&self.client, StatusFilter::Open).await {
            Ok(r) => r,
            Err(e) => return Ok(fail(&e)),
        };
        let date = p
            .delivery_date
            .unwrap_or_else(|| chrono::Local::now().format("%m/%d/%Y").to_string());

        let mut session = match self.store.current() {
            Ok(Some(mut existing)) => {
                if let Err(why) = existing.touch_for_edit() {
                    return Ok(ok(why));
                }
                existing.delivery_date = date.clone();
                existing
            }
            _ => DeliverySession::new(
                date.clone(),
                chrono::Local::now().to_rfc3339(),
            ),
        };

        let mut seen: Vec<u64> = Vec::new();
        let mut groups = Vec::new();
        for g in p.groups {
            let mut deliveries = Vec::new();
            for d in g.deliveries {
                let Some(r) = open.iter().find(|r| r.id == d.request_id) else {
                    return Ok(ok(format!(
                        "Request {} is not in the list of open requests. It may already \n                         have been recorded. Run list_open_requests and try again.",
                        d.request_id
                    )));
                };
                if seen.contains(&d.request_id) {
                    return Ok(ok(format!(
                        "{} appears in more than one volunteer group. Each family \n                         belongs to exactly one group -- which one delivered to them?",
                        r.display_name()
                    )));
                }
                seen.push(d.request_id);
                deliveries.push(Delivery {
                    request_id: r.id,
                    // Only the list carries clientId, and it excludes completed
                    // requests -- so capture it now, while the request is open.
                    client_id: r.client.id,
                    name: r.display_name(),
                    household_size: r.calculated_household_count,
                    gift_card_dollars: d.gift_card_dollars,
                    food_dollars: d
                        .food_dollars
                        .unwrap_or_else(|| self.config.second_harvest.value.unwrap_or(70)),
                    version: Some(r.version),
                    outcome: DeliveryOutcome::Delivered,
                    food: SlotState::Pending,
                    gift_card: SlotState::Pending,
                    complete: SlotState::Pending,
                });
            }
            groups.push(Group {
                volunteer_id: g.volunteer_id,
                volunteer_name: g.volunteer_name,
                deliveries,
            });
        }

        if groups.iter().all(|g| g.deliveries.is_empty()) {
            return Ok(ok("No deliveries in the plan yet. Which families were \n                          delivered to today?"));
        }
        session.groups = groups;
        if let Err(e) = self.store.save(&session) {
            return Ok(CallToolResult::error(vec![ContentBlock::text(e.to_string())]));
        }
        Ok(ok(format!(
            "{}\n\nIf that is right, confirm it with session id {} and revision {}.",
            render_plan(&session, &self.config),
            session.id,
            session.revision
        )))
    }

    /// Records the volunteer's approval of the plan. Must be called, with the
    /// plan shown to them first, before anything can be saved to ServWare.
    #[tool(name = "confirm_plan")]
    async fn confirm_plan(
        &self,
        Parameters(p): Parameters<ConfirmParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let mut session = match self.store.load(&p.session_id) {
            Ok(s) => s,
            Err(e) => return Ok(ok(format!("{e}"))),
        };
        if session.revision != p.revision {
            return Ok(ok(format!(
                "The plan changed after that version (you have {}, it is now {}). \n                 Show the volunteer this plan and confirm again:\n\n{}",
                p.revision,
                session.revision,
                render_plan(&session, &self.config)
            )));
        }
        session.state = SessionState::Confirmed;
        if let Err(e) = self.store.save(&session) {
            return Ok(CallToolResult::error(vec![ContentBlock::text(e.to_string())]));
        }
        Ok(ok(format!(
            "Confirmed. Save it to ServWare with submit_session and session id {}.",
            session.id
        )))
    }

    /// Saves the confirmed plan to ServWare. Safe to run again if something
    /// fails part-way: it checks what is already recorded and never repeats it.
    #[tool(name = "submit_session")]
    async fn submit_session(
        &self,
        Parameters(p): Parameters<SessionIdParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let mut session = match self.store.load(&p.session_id) {
            Ok(s) => s,
            Err(e) => return Ok(ok(format!("{e}"))),
        };
        let backend = ServWareBackend { client: &self.client, config: &self.config };
        let now = chrono::Local::now().to_rfc3339();

        let report = match submit(&backend, &mut session, &self.config, &now).await {
            Ok(r) => r,
            Err(why) => return Ok(ok(why)),
        };
        // Save the receipt even if writes failed: it is the record of what did
        // reach ServWare, and resuming depends on it.
        if let Err(e) = self.store.save(&session) {
            tracing::error!(%e, "could not save the delivery receipt");
        }

        let mut out = Vec::new();
        if report.is_clean() {
            out.push(format!(
                "Saved. {} families recorded, ${} logged in total.",
                session.deliveries().count(),
                session.total_dollars()
            ));
            if report.skipped > 0 {
                out.push(format!(
                    "({} entries were already in ServWare and were left alone.)",
                    report.skipped
                ));
            }
        } else {
            out.push("Some entries did not go through:".into());
            out.extend(report.attention.iter().map(|a| format!("  - {a}")));
            out.push(String::new());
            out.push(
                "Nothing was lost and nothing was recorded twice. Running                  submit_session again will pick up only what is missing."
                    .into(),
            );
        }
        Ok(ok(out.join("\n")))
    }

    /// Shows the delivery being recorded and how much of it has been saved.
    #[tool(name = "get_session", annotations(read_only_hint = true))]
    async fn get_session(&self) -> Result<CallToolResult, ErrorData> {
        match self.store.current() {
            Ok(Some(s)) => Ok(ok(format!(
                "{}\n\nStatus: {:?}. Session id {}, revision {}.",
                render_plan(&s, &self.config),
                s.state,
                s.id,
                s.revision
            ))),
            Ok(None) => Ok(ok("No delivery is being recorded right now.")),
            Err(e) => Ok(CallToolResult::error(vec![ContentBlock::text(e.to_string())])),
        }
    }

    /// Discards a delivery plan that has not been saved to ServWare.
    #[tool(name = "abandon_session")]
    async fn abandon_session(
        &self,
        Parameters(p): Parameters<SessionIdParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let mut session = match self.store.load(&p.session_id) {
            Ok(s) => s,
            Err(e) => return Ok(ok(format!("{e}"))),
        };
        if session.has_written() {
            return Ok(ok(
                "Part of this delivery is already saved in ServWare, so it cannot be \n                 discarded here. Finish saving it, or correct it in the ServWare website."
                    .to_string(),
            ));
        }
        session.state = SessionState::Abandoned;
        if let Err(e) = self.store.save(&session) {
            return Ok(CallToolResult::error(vec![ContentBlock::text(e.to_string())]));
        }
        Ok(ok("Discarded. Nothing was saved to ServWare."))
    }
}

impl Svdp {
    fn project(&self, r: &RequestSummary, today: chrono::NaiveDate) -> OpenRequestView {
        let phone = if r.client.mobile_phone.trim().is_empty() {
            r.client.home_phone.trim()
        } else {
            r.client.mobile_phone.trim()
        };
        OpenRequestView {
            request_id: r.id,
            name: r.display_name(),
            household_size: r.calculated_household_count,
            address: r.address(),
            phone: phone.to_string(),
            date_requested: r.date_requested.clone(),
            days_open: days_since(&r.date_requested, today),
            suggested_gift_card_dollars: self
                .config
                .gift_card_dollars(r.calculated_household_count),
        }
    }
}

/// ServWare renders dates as `MM/DD/YYYY`.
fn days_since(date: &str, today: chrono::NaiveDate) -> i64 {
    chrono::NaiveDate::parse_from_str(date.trim(), "%m/%d/%Y")
        .map(|d| (today - d).num_days())
        .unwrap_or(0)
}

fn render_table(views: &[OpenRequestView]) -> String {
    if views.is_empty() {
        return "There are no open requests in ServWare right now.".into();
    }
    let mut out = format!("{} open requests, longest-waiting first:\n\n", views.len());
    out.push_str("| # | Name | Household | Gift card | Requested | Waiting | Address | Phone |\n");
    out.push_str("|---|------|-----------|-----------|-----------|---------|---------|-------|\n");
    for v in views {
        out.push_str(&format!(
            "| {} | {} | {} | ${} | {} | {} days | {} | {} |\n",
            v.request_id,
            v.name,
            v.household_size,
            v.suggested_gift_card_dollars,
            v.date_requested,
            v.days_open,
            v.address,
            v.phone
        ));
    }
    out
}

/// The plan as a volunteer should see it: names, amounts, totals, no ids.
///
/// This is what gets read aloud before the only irreversible step, so it is
/// deliberately plain -- and it shows the food amount explicitly so the standard
/// $70 is never a surprise.
fn render_plan(session: &DeliverySession, _config: &ConferenceConfig) -> String {
    let mut out = format!("Delivery on {}:\n", session.delivery_date);
    for group in &session.groups {
        if group.deliveries.is_empty() {
            continue;
        }
        out.push_str(&format!("\n{} delivered to:\n", group.volunteer_name));
        for d in &group.deliveries {
            let status = match (&d.food, &d.gift_card, &d.complete) {
                (f, g, c) if f.is_done() && g.is_done() && c.is_done() => "  (saved)",
                (f, g, c)
                    if f.needs_attention() || g.needs_attention() || c.needs_attention() =>
                {
                    "  (needs attention)"
                }
                _ => "",
            };
            out.push_str(&format!(
                "  {} — ${} in gift cards, ${} of food{}\n",
                d.name, d.gift_card_dollars, d.food_dollars, status
            ));
        }
    }
    out.push_str(&format!(
        "\n{} families, ${} in total.",
        session.deliveries().count(),
        session.total_dollars()
    ));
    out
}

fn ok(text: impl Into<String>) -> CallToolResult {
    CallToolResult::success(vec![ContentBlock::text(text)])
}

/// Errors reach the volunteer as plain English; the detail goes to the log.
fn fail(e: &ServWareError) -> CallToolResult {
    tracing::warn!(error = %e, "tool call failed");
    CallToolResult::error(vec![ContentBlock::text(e.user_message())])
}

/// Starting points a client can offer the user directly, without them having to
/// phrase a request. Where the client surfaces these (a slash command, a menu),
/// this is the shortest path from "I did deliveries" to the workflow running.
#[prompt_router]
impl Svdp {
    /// Record today's SVdP deliveries in ServWare
    #[prompt(name = "record_deliveries")]
    async fn record_deliveries_prompt(&self) -> Result<Vec<PromptMessage>, ErrorData> {
        Ok(vec![PromptMessage::new_text(
            rmcp::model::Role::User,
            "I did SVdP deliveries today and need to record them in ServWare.              Please start by checking that ServWare is reachable, then show me              the families who are waiting so I can say which ones we delivered to.",
        )])
    }

    /// Show the SVdP families still waiting for a delivery
    #[prompt(name = "who_is_waiting")]
    async fn who_is_waiting_prompt(&self) -> Result<Vec<PromptMessage>, ErrorData> {
        Ok(vec![PromptMessage::new_text(
            rmcp::model::Role::User,
            "Show me the SVdP families with open requests, longest waiting first,              with the gift card amount each household size calls for.",
        )])
    }

    /// Check on a delivery that was not finished being saved
    #[prompt(name = "finish_saving")]
    async fn finish_saving_prompt(&self) -> Result<Vec<PromptMessage>, ErrorData> {
        Ok(vec![PromptMessage::new_text(
            rmcp::model::Role::User,
            "Check whether there is an SVdP delivery that was not finished being              saved to ServWare, and if so, tell me what is left and offer to finish it.",
        )])
    }
}

#[tool_handler]
#[prompt_handler]
impl ServerHandler for Svdp {
    fn get_info(&self) -> ServerInfo {
        let mut info = ServerInfo::new(ServerCapabilities::builder().enable_tools().enable_prompts().build());
        info.instructions = Some(
            "Tools for recording St. Vincent de Paul food and gift card deliveries in \
             ServWare. Run servware_health first. Speak plainly: the people using this \
             are volunteers, often elderly, not computer users."
                .into(),
        );
        info
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // stdout is the MCP transport, so logs must go to stderr. Default to warn so
    // neighbour details do not accumulate in Claude Desktop's log files.
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .init();

    let credentials = Credentials::from_env().ok_or_else(|| {
        anyhow::anyhow!(
            "ServWare username and password are not configured. Open the extension's \
             settings in Claude Desktop and fill them in."
        )
    })?;
    let base = std::env::var("SERVWARE_BASE_URL")
        .unwrap_or_else(|_| PUBLIC_BASE_URL.to_string());

    let server = Svdp {
        client: Arc::new(ServWareClient::new(&base, credentials)?),
        config: Arc::new(ConferenceConfig::load()),
        store: Arc::new(SessionStore::open_default()?),
    };
    server.serve(stdio()).await?.waiting().await?;
    Ok(())
}
