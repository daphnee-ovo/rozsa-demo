use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// OAuth credentials stored in auth.json.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthCredentials {
    pub access: String,
    pub refresh: String,
    pub expires: i64, // milliseconds since epoch
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// Callback events emitted during an OAuth login flow.
/// These are sent from the login implementation to the bridge event channel.
#[derive(Debug, Clone)]
pub enum OAuthFlowEvent {
    /// Show an authorization URL for the user to open in their browser.
    AuthUrl {
        url: String,
        instructions: Option<String>,
    },
    /// Show a device code for the user to enter at the verification URI.
    DeviceCode {
        user_code: String,
        verification_uri: String,
    },
    /// Request text input from the user (e.g., enterprise domain, manual code paste).
    Prompt {
        message: String,
        placeholder: Option<String>,
    },
    /// Request a selection from the user.
    Select {
        message: String,
        options: Vec<String>,
    },
    /// Progress update (e.g., "Polling for authorization...").
    Progress { message: String },
    /// Waiting/polling indicator.
    Waiting { message: String },
}

/// Result of a completed OAuth login flow.
pub type OAuthLoginResult = Result<OAuthCredentials, OAuthLoginError>;

#[derive(Debug, thiserror::Error)]
pub enum OAuthLoginError {
    #[error("login cancelled")]
    Cancelled,
    #[error("login timed out")]
    Timeout,
    #[error("token exchange failed: {0}")]
    TokenExchange(String),
    #[error("network error: {0}")]
    Network(String),
    #[error("callback server error: {0}")]
    CallbackServer(String),
    #[error("provider error: {0}")]
    Provider(String),
}
