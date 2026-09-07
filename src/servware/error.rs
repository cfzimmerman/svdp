//! Typed protocol errors.
//!
//! The MCP layer matches on these to decide retry / skip / escalate, which is
//! why this is not `anyhow`. See DECISIONS.md D11.

use crate::servware::form::FormError;

#[derive(Debug, thiserror::Error)]
pub enum ServWareError {
    /// The extension was installed but its username and password boxes are empty.
    /// Distinct from `LoginFailed`: nothing was rejected, nothing was tried.
    #[error("the ServWare username and password have not been filled in")]
    NotConfigured,

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

    /// The request was legitimate but asks for more than this will fetch in one
    /// go. Distinct from `Malformed` because nothing is wrong: the message is
    /// guidance for the person, so it reaches them verbatim rather than being
    /// replaced by "something went wrong".
    #[error("{0}")]
    TooBroad(String),

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
            Self::NotConfigured => SETUP_INSTRUCTIONS.into(),
            Self::LoginFailed => {
                "ServWare would not accept that username and password. Open Claude's \
                 Settings, go to Extensions, open SVdP ServWare, and check what is typed \
                 in the two boxes."
                    .into()
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
            // Already written for a volunteer to read, and it says what to do
            // next, so it must not be flattened into a generic failure.
            Self::TooBroad(msg) => msg.clone(),
            Self::Transport(_) => {
                "Could not reach ServWare. Check the internet connection.".into()
            }
        }
    }
}

/// Said the same way everywhere, because this is the first thing a volunteer
/// hits and the only place they can fix it. No jargon, no log file, no file path.
pub const SETUP_INSTRUCTIONS: &str = "\
This needs your ServWare username and password before it can do anything, and they \
have not been filled in yet.\n\n\
To add them:\n\
1. Open Claude's Settings (the gear icon).\n\
2. Click Extensions.\n\
3. Click SVdP ServWare.\n\
4. Type your ServWare username and password into the two boxes.\n\
5. Make sure the switch next to it is turned on.\n\n\
These are the same username and password you use on servware.org. Your conference \
president issues them. They are stored by your own computer and never appear in this \
conversation.";

pub type Result<T> = std::result::Result<T, ServWareError>;
