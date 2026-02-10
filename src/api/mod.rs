pub mod fetch_members;
pub mod fetch_requests;
pub mod update_assistance;
pub mod update_request;

use std::sync::Arc;

use anyhow::Context;
use anyhow::bail;
use reqwest::redirect;
use secrecy::ExposeSecret;
use secrecy::SecretString;

const BASE_URL: &str = "https://www.servware.org";

// ---------------------------------------------------------------------------
// ServWare client
// ---------------------------------------------------------------------------

/// Authenticated ServWare API client. Holds a cookie-jar-backed HTTP client
/// so that session cookies are automatically managed.
pub struct ServWare {
    pub(crate) client: reqwest::Client,
}

impl ServWare {
    fn login_url() -> String {
        format!("{BASE_URL}/security/login")
    }

    fn list_url() -> String {
        format!("{BASE_URL}/app/assistancerequests/list")
    }

    fn request_url(id: u64) -> String {
        format!("{BASE_URL}/app/assistancerequests/{id}")
    }

    fn assistance_item_url(id: u64) -> String {
        format!("{BASE_URL}/app/assistancerequests/{id}/assistanceitems/new")
    }

    fn extend_session_url() -> String {
        format!("{BASE_URL}/security/extendSession")
    }
}

impl ServWare {
    /// Authenticate with ServWare and return a new session.
    pub async fn new_session(username: &str, password: &SecretString) -> anyhow::Result<Self> {
        let jar = Arc::new(reqwest::cookie::Jar::default());
        let client = reqwest::Client::builder()
            .cookie_provider(jar)
            .redirect(redirect::Policy::limited(10))
            .user_agent("svdp-client/0.1")
            .build()
            .context("failed to build HTTP client")?;

        let url = Self::login_url();
        tracing::debug!(%url, %username, "attempting login");

        let params = [
            ("username", username),
            ("password", password.expose_secret()),
        ];

        let response = client
            .post(&url)
            .form(&params)
            .send()
            .await
            .context("login request failed")?;

        let final_url = response.url().to_string();
        let status = response.status();
        tracing::debug!(%status, %final_url, "login response");

        // A failed login redirects back to the login page.
        if final_url.contains("/security/login") {
            bail!("login failed: redirected back to login page (bad credentials?)");
        }

        if !status.is_success() {
            bail!("login failed with status {status}");
        }

        tracing::info!("logged in successfully");
        Ok(Self { client })
    }

    /// Extend the current ServWare session to keep it alive.
    pub async fn extend_session(&self) -> anyhow::Result<()> {
        let url = Self::extend_session_url();
        tracing::debug!(%url, "extending session");

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("extend session request failed")?;

        let status = response.status();
        tracing::debug!(%status, "extend session response");

        if !status.is_success() {
            bail!("session extend failed with status {status} — session may have expired");
        }

        Ok(())
    }

    /// Ping ServWare by extending the session. Confirms the session is still active.
    pub async fn ping(&self) -> anyhow::Result<()> {
        self.extend_session().await
    }
}
