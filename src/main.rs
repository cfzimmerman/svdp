use anyhow::Context;
use svdp::api::ServWare;

use clap::Parser;
use clap::Subcommand;
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "svdp", about = "Admin tools for SVDP at Nativity")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Log in to ServWare and confirm authentication works.
    Login,
    /// Ping ServWare to check if the session is active.
    Ping,
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
        Command::Login => {
            tracing::info!("login successful");
        }
        Command::Ping => {
            client.ping().await?;
        }
    }

    Ok(())
}
