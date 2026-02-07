/// Base URL for the ServWare application.
pub const BASE_URL: &str = "https://servware.org";

// TODO: Update these after capturing network traffic from the browser.
pub const LOGIN_PATH: &str = "/app/clients/j_security_check";
pub const EXTEND_SESSION_PATH: &str = "/security/extendSession";

/// Build a full URL from a path.
pub fn url(path: &str) -> String {
    format!("{BASE_URL}{path}")
}
