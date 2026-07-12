use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Compaction settings
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompactionSettings {
    pub enabled: bool,
    pub threshold_tokens: u64,
    pub target_tokens: u64,
}

impl Default for CompactionSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            threshold_tokens: 16384,
            target_tokens: 20000,
        }
    }
}

/// Retry settings
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetrySettings {
    pub timeout_ms: Option<u64>,
    pub max_retries: Option<u32>,
    pub max_retry_delay_ms: Option<u64>,
}

impl Default for RetrySettings {
    fn default() -> Self {
        Self {
            timeout_ms: None,
            max_retries: Some(3),
            max_retry_delay_ms: Some(60000),
        }
    }
}

/// Permission settings
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionSettings {
    /// User-configured rules. Evaluation order is deny, ask, then allow.
    #[serde(default)]
    pub deny: Vec<String>,
    #[serde(default)]
    pub ask: Vec<String>,
    #[serde(default)]
    pub allow: Vec<String>,
    /// Legacy mode retained for settings compatibility. Explicit rules always
    /// take precedence over this fallback.
    pub mode: String,
    pub auto_approve_patterns: Vec<String>,
    pub allowed_tools: Vec<String>,
    pub blocked_commands: Vec<String>,
}

impl Default for PermissionSettings {
    fn default() -> Self {
        Self {
            deny: Vec::new(),
            ask: Vec::new(),
            allow: Vec::new(),
            mode: "on-request".to_string(),
            auto_approve_patterns: Vec::new(),
            allowed_tools: Vec::new(),
            blocked_commands: Vec::new(),
        }
    }
}

/// Fully resolved settings (all fields have values)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub default_provider: Option<String>,
    pub default_model: Option<String>,
    pub default_thinking_level: Option<rozsa_model::types::ThinkingLevel>,
    pub compaction: CompactionSettings,
    pub retry: RetrySettings,
    pub transport: String,
    pub block_images: bool,
    pub hide_thinking: bool,
    pub steering_mode: String,
    pub follow_up_mode: String,
    pub running_send_mode: String,
    #[serde(rename = "permission", alias = "permissions")]
    pub permissions: PermissionSettings,
    pub context_window_preferences: HashMap<String, u64>,
    pub lsp_mode: String,
    #[serde(default)]
    pub appearance: AppearanceSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            default_provider: None,
            default_model: None,
            default_thinking_level: None,
            compaction: CompactionSettings::default(),
            retry: RetrySettings::default(),
            transport: "auto".to_string(),
            block_images: false,
            hide_thinking: false,
            steering_mode: "one-at-a-time".to_string(),
            follow_up_mode: "one-at-a-time".to_string(),
            running_send_mode: "queue".to_string(),
            permissions: PermissionSettings::default(),
            context_window_preferences: HashMap::new(),
            lsp_mode: "disabled".to_string(),
            appearance: AppearanceSettings::default(),
        }
    }
}

/// Persistent GUI appearance preferences. Theme palette values live in the
/// user theme files under `~/.rozsa/themes/`; this struct stores only the
/// active mode, font size, and selected theme ids.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppearanceSettings {
    pub theme_mode: String,
    pub font_size: u8,
    pub light_theme: String,
    pub dark_theme: String,
}

impl Default for AppearanceSettings {
    fn default() -> Self {
        Self {
            theme_mode: "light".to_string(),
            font_size: 13,
            light_theme: "rozsa".to_string(),
            dark_theme: "rozsa-dark".to_string(),
        }
    }
}

impl AppearanceSettings {
    pub fn validate(&self) -> Result<(), String> {
        if !matches!(self.theme_mode.as_str(), "system" | "light" | "dark") {
            return Err(format!("invalid theme mode: {}", self.theme_mode));
        }
        if !(5..=50).contains(&self.font_size) {
            return Err(format!(
                "font size must be between 5 and 50: {}",
                self.font_size
            ));
        }
        for (label, id) in [
            ("light theme", &self.light_theme),
            ("dark theme", &self.dark_theme),
        ] {
            if id.is_empty()
                || !id
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
            {
                return Err(format!("invalid {label} id: {id}"));
            }
        }
        Ok(())
    }
}

/// Partial settings (all fields optional, used for overlay)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PartialSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_thinking_level: Option<rozsa_model::types::ThinkingLevel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compaction: Option<PartialCompactionSettings>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry: Option<PartialRetrySettings>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transport: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block_images: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hide_thinking: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub steering_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub follow_up_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub running_send_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "permission", alias = "permissions")]
    pub permissions: Option<PartialPermissionSettings>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_window_preferences: Option<HashMap<String, u64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lsp_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub appearance: Option<PartialAppearanceSettings>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PartialAppearanceSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_size: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub light_theme: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dark_theme: Option<String>,
}

/// Partial compaction settings
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PartialCompactionSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub threshold_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_tokens: Option<u64>,
}

/// Partial retry settings
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PartialRetrySettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_retries: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_retry_delay_ms: Option<u64>,
}

/// Partial permission settings
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PartialPermissionSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deny: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ask: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_approve_patterns: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_tools: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked_commands: Option<Vec<String>>,
}
