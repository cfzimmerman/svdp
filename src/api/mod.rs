pub mod fetch_requests;
pub mod update_assistance;
pub mod update_request;

use std::sync::Arc;

use anyhow::{Context, bail};
use reqwest::redirect;
use secrecy::{ExposeSecret, SecretString};
use tracing::{debug, info};

use fetch_requests::{AssistanceRequest, FetchRequestsParams};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

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
}

impl ServWare {
    /// Authenticate with ServWare and return a new session.
    pub async fn new_session(
        username: &str,
        password: &SecretString,
    ) -> anyhow::Result<Self> {
        let jar = Arc::new(reqwest::cookie::Jar::default());
        let client = reqwest::Client::builder()
            .cookie_provider(jar)
            .redirect(redirect::Policy::limited(10))
            .user_agent("svdp-client/0.1")
            .build()
            .context("failed to build HTTP client")?;

        let url = Self::login_url();
        debug!(%url, %username, "attempting login");

        let params = [("username", username), ("password", password.expose_secret())];

        let response = client
            .post(&url)
            .form(&params)
            .send()
            .await
            .context("login request failed")?;

        let final_url = response.url().to_string();
        let status = response.status();
        debug!(%status, %final_url, "login response");

        // A failed login redirects back to the login page.
        if final_url.contains("/security/login") {
            bail!("login failed: redirected back to login page (bad credentials?)");
        }

        if !status.is_success() {
            bail!("login failed with status {status}");
        }

        info!("logged in successfully");
        Ok(Self { client })
    }

    /// Fetch a single assistance request by ID.
    ///
    /// Internally fetches all requests (no status filter, large page size) and
    /// finds the matching one.
    pub(crate) async fn get_request_by_id(
        &self,
        id: u64,
    ) -> anyhow::Result<AssistanceRequest> {
        let params = FetchRequestsParams {
            filter_by_status: String::new(), // all statuses
            display_length: 5000,
            ..FetchRequestsParams::new_open()
        };

        let response = self
            .fetch_requests(&params)
            .await
            .context("failed to fetch requests for get_request_by_id")?;

        response
            .aa_data
            .into_iter()
            .find(|r| r.id == id)
            .with_context(|| format!("request {id} not found in fetched results"))
    }
}
