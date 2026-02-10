use std::path::PathBuf;

use anyhow::Context;
use clap::Parser;
use clap::Subcommand;
use tracing_subscriber::EnvFilter;

use svdp::api::ServWare;
use svdp::api::fetch_requests::FetchRequestsParams;
use svdp::nativity;

#[derive(Parser)]
#[command(name = "svdp", about = "Admin tools for SVDP at Nativity")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Fetches a list of open requests and writes them to a CSV.
    GetRequests {
        #[arg(short, long)]
        csv: PathBuf,
    },

    /// Lists volunteer members (ID and name) from ServWare.
    ListMembers,

    /// Marks all requests in a CSV as complete with volunteer and visit details.
    MarkComplete {
        #[arg(short, long)]
        csv: PathBuf,

        #[arg(short = 'i', long)]
        member_id: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();
    let credentials = svdp::get_credentials().context("failed to get credentials")?;
    let client = ServWare::new_session(&credentials.username, &credentials.password).await?;

    match cli.command {
        Command::GetRequests { csv } => {
            nativity::requests_to_csv(&client, &csv).await?;
        }
        Command::ListMembers => {
            let reqs = client
                .fetch_requests(&FetchRequestsParams::new_open_asc())
                .await
                .context("failed to fetch open requests")?;
            let first = reqs
                .aa_data
                .first()
                .context("no open requests found to scrape member list from")?;
            let request_id = first.id;

            tracing::debug!(request_id, "using request to scrape member list");
            let members = client.fetch_members(request_id).await?;
            for m in &members {
                println!("{}\t{}", m.id, m.name);
            }
        }
        Command::MarkComplete { csv, member_id } => {
            nativity::mark_csv_complete(&client, &csv, &member_id).await?;
        }
    }

    Ok(())
}
