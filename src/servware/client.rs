//! Authenticated ServWare HTTP client.
//!
//! The base URL is injected rather than a const so tests can point at a local
//! server and exercise real cookies, redirects and form encoding. See
//! DECISIONS.md D10 for why the seam is here and not behind a mocked transport.

use std::sync::Arc;

use reqwest::Url;
use reqwest::header;
use reqwest::header::HeaderMap;
use reqwest::header::HeaderValue;
use reqwest::redirect;
use secrecy::ExposeSecret;
use secrecy::SecretString;

use crate::servware::error::Result;
use crate::servware::error::ServWareError;

pub const PUBLIC_BASE_URL: &str = "https://www.servware.org";

const USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 \
                          (KHTML, like Gecko) Chrome/145.0.0.0 Safari/537.36";

pub struct Credentials {
    pub username: String,
    pub password: SecretString,
}

impl Credentials {
    pub const USER_ENV: &'static str = "SERVWARE_USER";
    pub const PASS_ENV: &'static str = "SERVWARE_PASS";

    /// Read credentials from the environment.
    ///
    /// Under the `.mcpb` extension these are injected by Claude Desktop from its
    /// encrypted user config; the model never sees them.
    pub fn from_env() -> Option<Self> {
        let username = std::env::var(Self::USER_ENV).ok()?;
        let password = std::env::var(Self::PASS_ENV).ok()?;
        if username.trim().is_empty() || password.is_empty() {
            return None;
        }
        Some(Self {
            username: username.trim().to_string(),
            password: SecretString::new(password.into_boxed_str()),
        })
    }
}

pub struct ServWareClient {
    http: reqwest::Client,
    base: Url,
    credentials: Credentials,
}

impl ServWareClient {
    pub fn new(base_url: &str, credentials: Credentials) -> Result<Self> {
        let base = Url::parse(base_url)
            .map_err(|e| ServWareError::Malformed(format!("bad base url: {e}")))?;

        let mut headers = HeaderMap::new();
        headers.insert(
            header::ACCEPT,
            HeaderValue::from_static(
                "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
            ),
        );
        headers.insert(header::ACCEPT_LANGUAGE, HeaderValue::from_static("en-US,en;q=0.9"));

        let http = reqwest::Client::builder()
            .cookie_provider(Arc::new(reqwest::cookie::Jar::default()))
            .redirect(redirect::Policy::limited(10))
            .user_agent(USER_AGENT)
            .default_headers(headers)
            .build()?;

        Ok(Self { http, base, credentials })
    }

    pub fn url(&self, path: &str) -> Result<Url> {
        self.base
            .join(path)
            .map_err(|e| ServWareError::Malformed(format!("bad path {path}: {e}")))
    }

    /// Authenticate. A failed login redirects back to the login page rather than
    /// returning an error status, so the final URL is the signal.
    /// Whether credentials were supplied at all.
    pub fn is_configured(&self) -> bool {
        !self.credentials.username.trim().is_empty()
    }

    pub async fn login(&self) -> Result<()> {
        // Every ServWare call funnels through here, so one check covers all of
        // them. The server deliberately starts without credentials so it can say
        // this out loud; exiting instead put the explanation in a log file.
        if !self.is_configured() {
            return Err(ServWareError::NotConfigured);
        }
        let url = self.url("/security/login")?;
        let params = [
            ("username", self.credentials.username.as_str()),
            ("password", self.credentials.password.expose_secret()),
        ];
        let response = self
            .http
            .post(url.clone())
            .header(header::ORIGIN, self.base.as_str().trim_end_matches('/'))
            .header(header::REFERER, url.as_str())
            .form(&params)
            .send()
            .await?;

        if is_login_page(response.url().as_str()) {
            return Err(ServWareError::LoginFailed);
        }
        if !response.status().is_success() {
            return Err(ServWareError::LoginFailed);
        }
        tracing::debug!("signed in to ServWare");
        Ok(())
    }

    /// GET a page, re-authenticating once if the session has expired.
    pub async fn get_html(&self, path: &str) -> Result<String> {
        match self.try_get_html(path).await {
            Err(e) if e.is_retryable() => {
                tracing::info!("session expired; signing back in");
                self.login().await?;
                self.try_get_html(path).await
            }
            other => other,
        }
    }

    async fn try_get_html(&self, path: &str) -> Result<String> {
        let response = self.http.get(self.url(path)?).send().await?;
        if is_login_page(response.url().as_str()) {
            return Err(ServWareError::SessionExpired);
        }
        if !response.status().is_success() {
            return Err(ServWareError::Malformed(format!(
                "GET {path} returned {}",
                response.status()
            )));
        }
        Ok(response.text().await?)
    }

    /// GET a JSON endpoint, re-authenticating once if the session has expired.
    ///
    /// An expired session answers an XHR with the login *page*, so a JSON parse
    /// failure here is usually really an auth failure and is reported as one.
    pub async fn get_json(&self, path_and_query: &str) -> Result<serde_json::Value> {
        match self.try_get_json(path_and_query).await {
            Err(e) if e.is_retryable() => {
                self.login().await?;
                self.try_get_json(path_and_query).await
            }
            other => other,
        }
    }

    async fn try_get_json(&self, path_and_query: &str) -> Result<serde_json::Value> {
        let response = self
            .http
            .get(self.url(path_and_query)?)
            .header("X-Requested-With", "XMLHttpRequest")
            .header(header::ACCEPT, "application/json, text/javascript, */*; q=0.01")
            .send()
            .await?;

        if is_login_page(response.url().as_str()) {
            return Err(ServWareError::SessionExpired);
        }
        let status = response.status();
        let body = response.text().await?;
        if !status.is_success() {
            return Err(ServWareError::Malformed(format!("{path_and_query} returned {status}")));
        }
        if body.trim_start().starts_with('<') {
            // HTML where JSON was expected: almost always the login page.
            return Err(ServWareError::SessionExpired);
        }
        serde_json::from_str(&body)
            .map_err(|e| ServWareError::Malformed(format!("{path_and_query}: {e}")))
    }

    /// POST a form. **The response is not evidence of success** — Spring
    /// re-renders a rejected form as 200. Callers must verify by read-back.
    pub async fn post_form(&self, path: &str, pairs: &[(String, String)]) -> Result<()> {
        let url = self.url(path)?;
        let response = self
            .http
            .post(url.clone())
            .header(header::ORIGIN, self.base.as_str().trim_end_matches('/'))
            .header(header::REFERER, url.as_str())
            .form(pairs)
            .send()
            .await?;

        if is_login_page(response.url().as_str()) {
            return Err(ServWareError::SessionExpired);
        }
        let status = response.status();
        if !status.is_success() && !status.is_redirection() {
            return Err(ServWareError::Malformed(format!("POST {path} returned {status}")));
        }
        Ok(())
    }
}

fn is_login_page(url: &str) -> bool {
    url.contains("/security/login")
}
