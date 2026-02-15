use std::path::PathBuf;

use anyhow::Context;
use clap::Parser;
use clap::Subcommand;
use svdp::Credentials;
use tracing_subscriber::EnvFilter;

use svdp::api::ServWare;
use svdp::nativity;

#[derive(Parser)]
#[command(name = "svdp", about = "Admin tools for SVDP at Nativity")]
pub struct Args {
    #[command(subcommand)]
    pub command: Command,

    #[arg(short, long, default_value_t = false)]
    search_env: bool,
}

#[derive(Subcommand)]
pub enum Command {
    /// Lists volunteer members (ID and name) from ServWare.
    ListMembers {
        #[arg(short, long, default_value = "volunteers.csv")]
        csv: PathBuf,
    },

    /// Fetches a list of open requests and writes them to a CSV.
    GetRequests {
        #[arg(short, long, default_value = "requests.csv")]
        csv: PathBuf,
    },

    /// Marks all requests in a CSV as complete with volunteer and visit details.
    MarkComplete {
        #[arg(short, long, default_value = "requests.csv")]
        csv: PathBuf,

        #[arg(short = 'i', long)]
        volunteer_id: String,
    },

    /// Adds Second Harvest food and gift card assistance items to each request in the CSV.
    AddAssistance {
        #[arg(short, long, default_value = "requests.csv")]
        csv: PathBuf,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().or_else(|_| EnvFilter::try_new("INFO"))?)
        .init();

    let args = Args::parse();
    let credentials = Credentials::prompt(args.search_env).context("failed to get credentials")?;
    let client = ServWare::new_session(&credentials.username, &credentials.password).await?;

    match args.command {
        Command::GetRequests { csv } => {
            nativity::requests_to_csv(&client, &csv).await?;
        }
        Command::ListMembers { csv } => {
            nativity::members_to_csv(&client, &csv).await?;
        }
        Command::MarkComplete { csv, volunteer_id } => {
            nativity::update_complete(&client, &csv, &volunteer_id).await?;
        }
        Command::AddAssistance { csv } => {
            nativity::add_assistance(&client, &csv).await?;
        }
    }

    Ok(())
}
