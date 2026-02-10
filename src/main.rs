use std::path::PathBuf;

use anyhow::Context;
use clap::Parser;
use clap::Subcommand;
use tracing_subscriber::EnvFilter;

use svdp::api::ServWare;
use svdp::nativity;

#[derive(Parser)]
#[command(name = "svdp", about = "Admin tools for SVDP at Nativity")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Fetches a list of open requests and writes them to a
    GetRequests { csv: PathBuf },
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
    }

    Ok(())
}
