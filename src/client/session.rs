use anyhow::{Context, bail};
use reqwest::Client;
use tracing::debug;

use super::endpoints;

/// Extend the current ServWare session to keep it alive.
pub async fn extend_session(client: &Client) -> anyhow::Result<()> {
    let url = endpoints::url(endpoints::EXTEND_SESSION_PATH);
    debug!(%url, "extending session");

    let response = client
        .get(&url)
        .send()
        .await
        .context("extend session request failed")?;

    let status = response.status();
    debug!(%status, "extend session response");

    if !status.is_success() {
        bail!("session extend failed with status {status} — session may have expired");
    }

    Ok(())
}

/// Ping ServWare by extending the session. Confirms the session is still active.
pub async fn ping(client: &Client) -> anyhow::Result<()> {
    extend_session(client).await
}
