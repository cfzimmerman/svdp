//! `svdp` — maintainer CLI over the same logic layer the MCP server uses.
//!
//! Exists so failures can be read directly instead of through a chat client,
//! and so a delivery night has a fallback that is not the legacy code path.

use anyhow::Context;
use clap::Parser;
use clap::Subcommand;
use tracing_subscriber::EnvFilter;

use svdp::domain::policy::ConferenceConfig;
use svdp::servware::client::Credentials;
use svdp::servware::client::PUBLIC_BASE_URL;
use svdp::servware::client::ServWareClient;
use svdp::servware::detail;
use svdp::servware::list;
use svdp::servware::list::StatusFilter;

#[derive(Parser)]
#[command(name = "svdp", about = "Admin tools for SVdP at Nativity", version)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Check sign-in, the request list, and that ServWare's form still matches.
    Health,
    /// List open requests with suggested gift card amounts.
    Requests {
        /// Include completed requests too.
        #[arg(long)]
        all: bool,
    },
    /// List volunteers and their ServWare ids.
    Volunteers,
    /// Show one request and whatever is already logged against it.
    Request { id: u64 },
    /// Log ONE assistance item against ONE request. Dry-run unless --yes.
    AddItem {
        #[arg(long)]
        request: u64,
        /// `food` or `giftcard`.
        #[arg(long)]
        slot: String,
        #[arg(long)]
        dollars: u32,
        /// Session id, which becomes the idempotency tag in ServWare's notes.
        #[arg(long)]
        session: String,
        /// ServWare client id. Only discoverable from the request *list*, which
        /// excludes completed requests -- so pass it explicitly when re-running
        /// against a request that has already been closed.
        #[arg(long)]
        client_id: Option<u64>,
        /// Actually write. Without this, prints what would be sent and stops.
        #[arg(long)]
        r#yes: bool,
    },

    /// Mark ONE request complete and credit it to a volunteer. Dry-run unless --yes.
    Complete {
        #[arg(long)]
        request: u64,
        #[arg(long)]
        volunteer: String,
        #[arg(long)]
        r#yes: bool,
    },

    /// Save one live read to a local file so development can continue offline.
    ///
    /// ServWare is production; this exists so iteration costs one request rather
    /// than one per rebuild. Output is gitignored and holds real data.
    Snapshot {
        /// Request id whose detail page to capture. Defaults to the oldest open one.
        #[arg(long)]
        id: Option<u64>,
        #[arg(long, default_value = "recordings")]
        out: std::path::PathBuf,
    },
    /// Dump a request's edit form, to inspect what a write would send.
    Form {
        id: u64,
        /// Show only fields whose name contains this.
        #[arg(long)]
        filter: Option<String>,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // `.env` is loaded unconditionally. The old `-e` flag was pure friction:
    // there is no reason to have credentials on disk and not use them.
    let _ = dotenvy::dotenv();
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("svdp=info")),
        )
        .init();

    let args = Args::parse();
    let credentials = Credentials::from_env().context(
        "set SERVWARE_USER and SERVWARE_PASS, in the environment or in a .env file",
    )?;
    let base = std::env::var("SERVWARE_BASE_URL").unwrap_or_else(|_| PUBLIC_BASE_URL.into());
    let client = ServWareClient::new(&base, credentials)?;
    let config = ConferenceConfig::load();

    client.login().await.context("sign-in failed")?;

    match args.command {
        Command::Health => {
            let open = list::fetch_all(&client, StatusFilter::Open).await?;
            println!("signed in; {} open requests", open.len());
            if let Some(first) = open.first() {
                let d = detail::fetch(&client, first.id).await?;
                println!(
                    "request {} is {}; {} volunteers; {} assistance items",
                    d.id,
                    d.status(),
                    d.members.len(),
                    d.assistance_items.len()
                );
                let mut missing = Vec::new();
                for field in [
                    "status", "visitCompleted", "visitAssignedToMemberId",
                    "requestAssignedToMemberId", "homeVisitCnt", "visitMileageInService",
                    "visitScheduledDate", "visitNotes", "homeVisitRequired",
                ] {
                    if !d.form.contains(field) {
                        missing.push(field);
                    }
                }
                if missing.is_empty() {
                    println!("all write targets present in the form");
                } else {
                    println!("MISSING write targets: {missing:?}");
                }
            }
            println!(
                "gift cards ${}-${}; food ${}",
                config.gift_card_dollars(1),
                config.gift_card_dollars(99),
                config.second_harvest.value.unwrap_or(0)
            );
        }

        Command::Requests { all } => {
            let filter = if all { StatusFilter::Any } else { StatusFilter::Open };
            let requests = list::fetch_all(&client, filter).await?;
            println!("{} requests", requests.len());
            println!("{:>9}  {:<24} {:>3} {:>6}  {:<10} {}", "ID", "NAME", "HH", "CARD", "REQUESTED", "STATUS");
            for r in &requests {
                println!(
                    "{:>9}  {:<24} {:>3} {:>5}$  {:<10} {}",
                    r.id,
                    truncate(&r.display_name(), 24),
                    r.calculated_household_count,
                    config.gift_card_dollars(r.calculated_household_count),
                    r.date_requested,
                    r.status,
                );
            }
        }

        Command::Volunteers => {
            let open = list::fetch_all(&client, StatusFilter::Open).await?;
            let first = open.first().context("no open requests to read the volunteer list from")?;
            let d = detail::fetch(&client, first.id).await?;
            println!("{} volunteers", d.members.len());
            for m in &d.members {
                println!("{:>8}  {}", m.id, m.name);
            }
        }

        Command::Request { id } => {
            let d = detail::fetch(&client, id).await?;
            println!("request {} is {}", d.id, d.status());
            if d.assistance_items.is_empty() {
                println!("  nothing logged yet");
            }
            for item in &d.assistance_items {
                println!(
                    "  {:<28} ${:<8} {}  {}",
                    item.kind,
                    item.value,
                    item.date_provided,
                    item.tag.as_deref().unwrap_or("")
                );
            }
        }

        Command::AddItem { request, slot, dollars, session, client_id, yes } => {
            let slot = match slot.as_str() {
                "food" => svdp::domain::policy::Slot::Food,
                "giftcard" => svdp::domain::policy::Slot::GiftCard,
                other => anyhow::bail!("slot must be `food` or `giftcard`, not `{other}`"),
            };
            let date = chrono::Local::now().format("%m/%d/%Y").to_string();
            let before = detail::fetch(&client, request).await?;
            // `clientId` appears nowhere on the detail page; it comes only from
            // the list API, which is filtered by status. A completed request is
            // therefore unreachable that way, hence the explicit override.
            let client_id = match client_id {
                Some(id) => id,
                None => {
                    let open = list::fetch_all(&client, StatusFilter::Open).await?;
                    open.iter()
                        .find(|r| r.id == request)
                        .map(|r| r.client.id)
                        .context(
                            "could not determine the client id: this request is not in the \
                             open list (already completed?). Pass --client-id explicitly.",
                        )?
                }
            };
            let notes = config.item_notes(&session, slot, &date);
            println!("request {request} is currently {}", before.status());
            println!("already logged: {} items", before.assistance_items.len());
            println!("would POST to /app/assistancerequests/{request}/assistanceitems/new:");
            for (k, v) in svdp::servware::write::assistance_form(
                match slot {
                    svdp::domain::policy::Slot::Food => &config.second_harvest.id,
                    _ => &config.gift_card.id,
                },
                client_id,
                dollars,
                &date,
                &notes,
            ) {
                if !v.is_empty() {
                    println!("    {k:<24} = {v}");
                }
            }
            if !yes {
                println!("\nDRY RUN — nothing sent. Re-run with --yes to write.");
                return Ok(());
            }
            let outcome = svdp::servware::write::add_assistance_item(
                &client, &config, request, client_id, &session, slot, dollars, &date,
            )
            .await?;
            println!("\noutcome: {outcome:?}");
            let after = detail::fetch(&client, request).await?;
            println!("assistance items now: {}", after.assistance_items.len());
            for item in &after.assistance_items {
                println!("  {:<28} ${:<8} {}", item.kind, item.value, item.date_provided);
            }
        }

        Command::Complete { request, volunteer, yes } => {
            let date = chrono::Local::now().format("%m/%d/%Y").to_string();
            let before = detail::fetch(&client, request).await?;
            println!("request {request} is currently {}", before.status());
            let after_form = before.form.overlay([
                ("status", "Completed".to_string()),
                ("requestAssignedToMemberId", volunteer.clone()),
                ("visitAssignedToMemberId", volunteer.clone()),
                ("homeVisitRequired", "true".to_string()),
                ("homeVisitCnt", "1".to_string()),
                ("visitCompleted", "true".to_string()),
                ("visitMileageInService", config.visit_mileage.clone()),
                ("visitScheduledDate", date.clone()),
                ("visitNotes", config.visit_notes_html.clone()),
            ])?;
            println!("this would change exactly these fields:");
            for (name, before_v, after_v) in before.form.diff(&after_form) {
                println!("    {name:<34} {} -> {}", truncate(&before_v, 24), truncate(&after_v, 24));
            }
            println!("  ({} other fields preserved unchanged)", after_form.pairs().len());
            if !yes {
                println!("\nDRY RUN — nothing sent. Re-run with --yes to write.");
                return Ok(());
            }
            let outcome = svdp::servware::write::mark_complete(
                &client, &config, request, &volunteer, &date, None,
            )
            .await?;
            println!("\noutcome: {outcome:?}");
            println!("status now: {}", detail::fetch(&client, request).await?.status());
        }

        Command::Snapshot { id, out } => {
            std::fs::create_dir_all(&out)?;
            let id = match id {
                Some(id) => id,
                None => {
                    list::fetch_all(&client, StatusFilter::Open)
                        .await?
                        .first()
                        .context("no open requests")?
                        .id
                }
            };
            let html = client.get_html(&detail::detail_path(id)).await?;
            let path = out.join("detail.html");
            std::fs::write(&path, &html)?;
            println!("wrote {} ({} bytes)", path.display(), html.len());
            println!("this file contains real neighbour data and is gitignored");
            println!(
                "use it offline:  SVDP_LOCAL_DETAIL_HTML={} cargo test --test local_capture -- --ignored",
                path.display()
            );
        }

        Command::Form { id, filter } => {
            let d = detail::fetch(&client, id).await?;
            let pairs = d.form.pairs();
            println!("{} submitted controls", pairs.len());
            for (name, value) in pairs {
                if filter.as_ref().is_some_and(|f| !name.contains(f.as_str())) {
                    continue;
                }
                println!("  {name:<38} = {}", truncate(&value, 60));
            }
        }
    }

    Ok(())
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    s.chars().take(max.saturating_sub(1)).collect::<String>() + "…"
}
