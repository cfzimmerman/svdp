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
use rmcp::model::ServerInfo;
use rmcp::tool;
use rmcp::tool_handler;
use rmcp::tool_router;
use rmcp::transport::stdio;
use schemars::JsonSchema;
use serde::Deserialize;

use svdp::domain::policy::ConferenceConfig;
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

fn ok(text: impl Into<String>) -> CallToolResult {
    CallToolResult::success(vec![ContentBlock::text(text)])
}

/// Errors reach the volunteer as plain English; the detail goes to the log.
fn fail(e: &ServWareError) -> CallToolResult {
    tracing::warn!(error = %e, "tool call failed");
    CallToolResult::error(vec![ContentBlock::text(e.user_message())])
}

#[tool_handler]
impl ServerHandler for Svdp {
    fn get_info(&self) -> ServerInfo {
        let mut info = ServerInfo::new(ServerCapabilities::builder().enable_tools().build());
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
    };
    server.serve(stdio()).await?.waiting().await?;
    Ok(())
}
