//! Typed protocol errors.
//!
//! The MCP layer matches on these to decide retry / skip / escalate, which is
//! why this is not `anyhow`. See DECISIONS.md D11.

use crate::servware::form::FormError;

#[derive(Debug, thiserror::Error)]
pub enum ServWareError {
    #[error("ServWare rejected the sign-in (check the username and password)")]
    LoginFailed,

    /// The session expired. ServWare times out after an hour, and a delivery-night
    /// chat has long human gaps, so callers re-authenticate and retry once.
    #[error("ServWare signed you out; the session expired")]
    SessionExpired,

    #[error("request {0} was not found in ServWare")]
    NotFound(u64),

    /// ServWare's page no longer has a field this tool writes to. Loud on purpose:
    /// the alternative is silently posting a parameter the server ignores.
    #[error("ServWare's form has changed — this tool needs an update (field `{field}`). Nothing was written.")]
    FormChanged { field: String },

    /// A write was accepted at the HTTP level but did not take effect. Spring
    /// re-renders a rejected form as 200, so status codes cannot be trusted;
    /// this is raised by read-back verification.
    #[error("ServWare did not accept the change to request {request_id} ({what}). Nothing was written.")]
    WriteRejected { request_id: u64, what: String },

    /// Someone else changed the request between planning and submission.
    #[error("request {request_id} changed in ServWare since it was planned ({detail})")]
    Conflict { request_id: u64, detail: String },

    #[error("could not read ServWare's response: {0}")]
    Malformed(String),

    #[error("could not reach ServWare: {0}")]
    Transport(#[from] reqwest::Error),
}

impl From<FormError> for ServWareError {
    fn from(e: FormError) -> Self {
        match e {
            FormError::UnknownField(field) => Self::FormChanged { field },
            other => Self::Malformed(other.to_string()),
        }
    }
}

impl ServWareError {
    /// Whether re-authenticating and retrying once is worth attempting.
    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::SessionExpired)
    }

    /// A sentence safe to show a non-technical volunteer.
    ///
    /// Never surfaces a serde offset or an HTTP status; those are for the log.
    pub fn user_message(&self) -> String {
        match self {
            Self::LoginFailed => {
                "ServWare would not accept that username and password.".into()
            }
            Self::SessionExpired => "ServWare signed you out. Signing back in.".into(),
            Self::FormChanged { .. } => {
                "ServWare's page layout changed, so this tool needs an update. \
                 Nothing was written."
                    .into()
            }
            Self::WriteRejected { what, .. } => {
                format!("ServWare would not accept the {what}. Nothing was written.")
            }
            Self::Conflict { detail, .. } => {
                format!("Someone changed this request in ServWare since you planned it: {detail}")
            }
            Self::NotFound(id) => format!("Request {id} is no longer in ServWare."),
            Self::Malformed(_) => {
                "ServWare sent back something this tool did not understand. \
                 Nothing was written."
                    .into()
            }
            Self::Transport(_) => {
                "Could not reach ServWare. Check the internet connection.".into()
            }
        }
    }
}

pub type Result<T> = std::result::Result<T, ServWareError>;
