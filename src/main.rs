//! `svdp` — maintainer CLI over the same logic layer the MCP server uses.
//!
//! Exists so failures can be read directly instead of through a chat client,
//! and so a delivery night has a fallback that is not the legacy code path.

use anyhow::Context;
use clap::Parser;
use clap::Subcommand;
use tracing_subscriber::EnvFilter;

use svdp::domain::export;
use svdp::domain::export::ExportDir;
use svdp::domain::export::HouseholdRoster;
use svdp::domain::policy::ConferenceConfig;
use svdp::domain::pull;
use svdp::servware::client::Credentials;
use svdp::servware::client::PUBLIC_BASE_URL;
use svdp::servware::client::ServWareClient;
use svdp::servware::clients;
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
        /// Print full names. Off by default: this output lands in terminals,
        /// scrollback and agent transcripts, and CLAUDE.md's rule is that
        /// neighbour PII does not go there.
        #[arg(long)]
        full_names: bool,
    },
    /// List volunteers and their ServWare ids.
    Volunteers {
        /// Print full names rather than initials.
        #[arg(long)]
        full_names: bool,
    },
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
        /// Capture an arbitrary path instead, for protocol spikes.
        #[arg(long)]
        path: Option<String>,
        #[arg(long, default_value = "recordings")]
        out: std::path::PathBuf,
    },
    /// Export the whole neighbour roster to a CSV.
    ExportNeighbors {
        /// Where to write. Defaults to the Desktop.
        #[arg(long)]
        out: Option<std::path::PathBuf>,
    },
    /// Export assistance requests in a date range to a CSV.
    ExportRequests {
        /// MM/DD/YYYY. Omit to walk the whole history (bounded, but slow).
        #[arg(long)]
        from: Option<String>,
        /// MM/DD/YYYY.
        #[arg(long)]
        to: Option<String>,
        #[arg(long)]
        out: Option<std::path::PathBuf>,
    },
    /// Export household members and their ages. One page fetch per household.
    ExportHouseholdMembers {
        #[arg(long)]
        from: Option<String>,
        #[arg(long)]
        to: Option<String>,
        #[arg(long, default_value_t = pull::DEFAULT_MAX_HOUSEHOLDS)]
        max_households: u32,
        #[arg(long)]
        out: Option<std::path::PathBuf>,
    },
    /// List the exports already written.
    Exports {
        #[arg(long)]
        out: Option<std::path::PathBuf>,
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
            // Probe with `Any`, not `Open`. On a quiet week there is no open
            // request, and the check used to print "no open requests" and
            // report healthy having verified nothing at all.
            let probe = list::fetch_all(&client, StatusFilter::Any).await?;
            match probe.first() {
                None => println!("NOT CHECKED: ServWare has no requests to read a form from"),
                Some(first) => {
                    let d = detail::fetch(&client, first.id).await?;
                    println!(
                        "request {} is {}; {} volunteers; {} assistance items",
                        d.id,
                        d.status(),
                        d.members.len(),
                        d.assistance_items.len()
                    );
                    // `detail::fetch` already requires every completion field to
                    // be present, so reaching here means they all are.
                    println!(
                        "all {} write targets present in the form",
                        svdp::servware::write::COMPLETION_FIELDS.len()
                    );
                }
            }
            println!(
                "gift cards ${}-${}; food ${}",
                config.gift_card_dollars(1),
                config.gift_card_dollars(99),
                config.second_harvest.value.unwrap_or(0)
            );
        }

        Command::Requests { all, full_names } => {
            let filter = if all { StatusFilter::Any } else { StatusFilter::Open };
            let requests = list::fetch_all(&client, filter).await?;
            println!("{} requests", requests.len());
            println!("{:>9}  {:<24} {:>3} {:>6}  {:<10} STATUS", "ID", "NAME", "HH", "CARD", "REQUESTED");
            for r in &requests {
                println!(
                    "{:>9}  {:<24} {:>3} {:>5}$  {:<10} {}",
                    r.id,
                    truncate(&shorten_name(&r.display_name(), full_names), 24),
                    r.calculated_household_count,
                    config.gift_card_dollars(r.calculated_household_count),
                    r.date_requested,
                    r.status,
                );
            }
            if !full_names {
                println!("(names shortened; --full-names to show them in full)");
            }
        }

        Command::Volunteers { full_names } => {
            let any = list::fetch_all(&client, StatusFilter::Any).await?;
            let first = any.first().context("no requests to read the volunteer list from")?;
            let d = detail::fetch(&client, first.id).await?;
            println!("{} volunteers", d.members.len());
            for m in &d.members {
                println!("{:>8}  {}", m.id, shorten_name(&m.name, full_names));
            }
            if !full_names {
                println!("(names shortened; --full-names to show them in full)");
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
            if before.is_completed() {
                println!(
                    "it is already completed, so the write would do nothing and return \
                     AlreadyDone"
                );
            }
            // The same function the write calls. This preview used to re-declare
            // the overlay by hand and set `requestAssignedToMemberId`
            // unconditionally, while the write claims it only when the county's
            // slot is empty -- so the dry run described a change the write would
            // not make. See DECISIONS.md D33.
            let after_form =
                svdp::servware::write::plan_complete(&before, &config, &volunteer, &date)?;
            println!("this would change exactly these fields:");
            for (name, before_v, after_v) in before.form.diff(&after_form) {
                println!("    {name:<34} {} -> {}", truncate(&before_v, 24), truncate(&after_v, 24));
            }
            println!("  ({} other fields preserved unchanged)", after_form.pairs().len());
            if !yes {
                println!("\nDRY RUN — nothing sent. Re-run with --yes to write.");
                return Ok(());
            }
            let outcome =
                svdp::servware::write::mark_complete(&client, &config, request, &volunteer, &date)
                    .await?;
            println!("\noutcome: {outcome:?}");
            println!("status now: {}", detail::fetch(&client, request).await?.status());
        }

        Command::Snapshot { id, path, out } => {
            std::fs::create_dir_all(&out)?;
            if let Some(path) = path {
                let dest = out.join(format!("{}.html", path.trim_matches('/').replace('/', "_")));
                refuse_unless_ignored(&dest)?;
                let html = client.get_html(&path).await?;
                std::fs::write(&dest, &html)?;
                println!("wrote {} ({} bytes)", dest.display(), html.len());
                println!("this file contains real neighbour data; verified gitignored");
                return Ok(());
            }
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
            let path = out.join("detail.html");
            refuse_unless_ignored(&path)?;
            let html = client.get_html(&detail::detail_path(id)).await?;
            std::fs::write(&path, &html)?;
            println!("wrote {} ({} bytes)", path.display(), html.len());
            println!("this file contains real neighbour data; verified gitignored");
            println!(
                "use it offline:  SVDP_LOCAL_DETAIL_HTML={} cargo test --test local_capture -- --ignored",
                path.display()
            );
        }

        Command::ExportNeighbors { out } => {
            let rows = clients::fetch_all(&client).await?;
            let table = export::neighbors_table(&rows, chrono::Local::now().date_naive());
            write_export(&table, out)?;
        }

        Command::ExportRequests { from, to, out } => {
            let from = parse_arg_date(from.as_deref(), "--from")?;
            let to = parse_arg_date(to.as_deref(), "--to")?;
            let rows = list::fetch_window(&client, StatusFilter::Any, from, to).await?;
            write_export(&export::requests_table(&rows), out.clone())?;
            write_export(&export::assistance_table(&rows), out)?;
        }

        Command::ExportHouseholdMembers { from, to, max_households, out } => {
            let from = parse_arg_date(from.as_deref(), "--from")?;
            let to = parse_arg_date(to.as_deref(), "--to")?;
            let (households, stats) =
                pull::household_members(&client, from, to, max_households).await?;
            let rosters: Vec<HouseholdRoster<'_>> = households
                .iter()
                .map(|h| HouseholdRoster {
                    client_id: h.client_id,
                    household_last_name: &h.last_name,
                    members: &h.members,
                })
                .collect();
            let table = export::members_table(&rosters);
            println!(
                "{} households; {} requests made to ServWare; {} had no members listed",
                stats.households, stats.servware_requests, stats.without_members
            );
            if !stats.unreadable.is_empty() {
                println!(
                    "{} household pages could not be read and are NOT in this file: {:?}",
                    stats.unreadable.len(),
                    stats.unreadable
                );
            }
            write_export(&table, out)?;
        }

        Command::Exports { out } => {
            let dir = export_dir(out)?;
            println!("{}", dir.path().display());
            for (name, bytes) in dir.list() {
                println!("  {name}  ({bytes} bytes)");
            }
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

fn parse_arg_date(raw: Option<&str>, flag: &str) -> anyhow::Result<Option<chrono::NaiveDate>> {
    match raw {
        None => Ok(None),
        Some(s) => list::parse_date(s)
            .map(Some)
            .with_context(|| format!("{flag} must look like MM/DD/YYYY, got {s:?}")),
    }
}

fn export_dir(out: Option<std::path::PathBuf>) -> anyhow::Result<ExportDir> {
    match out {
        Some(dir) => Ok(ExportDir::at(dir)),
        None => Ok(ExportDir::desktop()?),
    }
}

fn write_export(table: &export::Table, out: Option<std::path::PathBuf>) -> anyhow::Result<()> {
    let dir = export_dir(out)?;
    let path = dir.write(table, chrono::Local::now().date_naive())?;
    println!("wrote {} ({} rows)", path.display(), table.len());
    println!("this file contains real neighbour data — keep it on this computer");
    Ok(())
}

/// Refuse to write real neighbour data anywhere git would track it.
///
/// `--out` is resolved against the process working directory while `.gitignore`
/// anchors `/recordings/` at the repository root, so `recordings/detail.html` was
/// ignored but `src/recordings/detail.html` was not -- and the command printed
/// "this file is gitignored" either way. Asking git is the only honest way to
/// know. This repository is public, so a leak here is permanent.
fn refuse_unless_ignored(path: &std::path::Path) -> anyhow::Result<()> {
    let out = std::process::Command::new("git")
        .args(["check-ignore", "--quiet", "--no-index"])
        .arg(path)
        .status();
    match out {
        Ok(status) if status.success() => Ok(()),
        Ok(_) => anyhow::bail!(
            "refusing to write {} — git does not ignore that path, and this file holds real \
             names, addresses, phone numbers, SSN last-4 and case notes. Write it under a \
             gitignored directory (./recordings/) or add the path to .gitignore first.",
            path.display()
        ),
        Err(e) => anyhow::bail!(
            "refusing to write {} — could not ask git whether it is ignored ({e}), and this \
             file holds real neighbour data.",
            path.display()
        ),
    }
}

/// "Ada Lovelace" -> "Ada L." Enough to recognise a household without putting a
/// full name into a terminal transcript.
fn shorten_name(name: &str, full: bool) -> String {
    if full {
        return name.to_string();
    }
    let mut parts = name.split_whitespace();
    let first = parts.next().unwrap_or_default();
    match parts.next_back().and_then(|s| s.chars().next()) {
        Some(initial) => format!("{first} {initial}."),
        None => first.to_string(),
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    s.chars().take(max.saturating_sub(1)).collect::<String>() + "…"
}
