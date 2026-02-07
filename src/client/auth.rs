use std::sync::Arc;

use anyhow::{Context, bail};
use reqwest::{Client, redirect};
use secrecy::{ExposeSecret, SecretString};
use tracing::{debug, info};

use super::endpoints;

/// Build an HTTP client with cookie jar and appropriate settings for ServWare.
pub fn build_http_client() -> anyhow::Result<Client> {
    let jar = Arc::new(reqwest::cookie::Jar::default());
    Client::builder()
        .cookie_provider(jar)
        .redirect(redirect::Policy::limited(10))
        .user_agent("svdp-client/0.1")
        .build()
        .context("failed to build HTTP client")
}

/// Authenticate with ServWare. Returns the authenticated client on success.
///
/// TODO: The form field names and login URL are placeholders.
/// Update after capturing the real login flow from the browser.
pub async fn login(client: &Client, username: &str, password: &SecretString) -> anyhow::Result<()> {
    let login_url = endpoints::url(endpoints::LOGIN_PATH);
    debug!(url = %login_url, username, "attempting login");

    // TODO: Update form field names after capturing the real login request.
    let params = [
        ("j_username", username),
        ("j_password", password.expose_secret()),
    ];

    let response = client
        .post(&login_url)
        .form(&params)
        .send()
        .await
        .context("login request failed")?;

    let status = response.status();
    debug!(%status, "login response");

    // After login, ServWare likely redirects. A successful login typically
    // returns 200 or 302 to a dashboard. An auth failure may redirect back
    // to the login page or return an error page.
    //
    // TODO: Refine this check after observing actual server behavior.
    if !status.is_success() && !status.is_redirection() {
        bail!("login failed with status {status}");
    }

    let body = response.text().await.context("failed to read login response body")?;

    // Check for common login failure indicators in the response body.
    // TODO: Refine after observing actual failure responses.
    if body.contains("login_error") || body.contains("Invalid credentials") {
        bail!("login failed: invalid credentials");
    }

    info!("logged in successfully");
    Ok(())
}
