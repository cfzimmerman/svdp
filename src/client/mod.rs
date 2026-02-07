mod auth;
pub mod endpoints;
mod session;

use anyhow::Context;
use reqwest::Client;
use secrecy::SecretString;
use tracing::info;

/// An authenticated ServWare HTTP client.
pub struct ServwareClient {
    http: Client,
}

impl ServwareClient {
    /// Log in to ServWare and return an authenticated client.
    pub async fn login(username: &str, password: &SecretString) -> anyhow::Result<Self> {
        let http = auth::build_http_client().context("failed to create HTTP client")?;
        auth::login(&http, username, password).await?;
        Ok(Self { http })
    }

    /// Ping ServWare to confirm the session is still active.
    pub async fn ping(&self) -> anyhow::Result<()> {
        session::ping(&self.http).await?;
        info!("session is active");
        Ok(())
    }

    /// Access the underlying HTTP client for making additional requests.
    pub fn http(&self) -> &Client {
        &self.http
    }
}
