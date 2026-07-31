// FrameworkTree
// schema.rs
// ├── struct CompactionSettings
// ├── impl CompactionSettings
// ├── default()
// ├── struct RetrySettings
// ├── impl RetrySettings
// ├── default()
// ├── struct PermissionSettings
// ├── impl PermissionSettings
// ├── default()
// ├── struct DevFlowSettings
// ├── impl DevFlowSettings
// ├── default()
// ├── struct Settings
// ├── impl Settings
// ├── default()
// ├── struct AppearanceSettings
// ├── impl AppearanceSettings
// ├── default()
// ├── default_quota_visibility()
// ├── impl AppearanceSettings
// ├── validate()
// ├── struct PartialSettings
// ├── struct PartialDevFlowSettings
// ├── struct PartialAppearanceSettings
// ├── struct PartialCompactionSettings
// ├── struct PartialRetrySettings
// └── struct PartialPermissionSettings

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

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
    /// Fallback mode used when no explicit rule applies. Explicit rules always
    /// take precedence.
    pub mode: String,
}

impl Default for PermissionSettings {
    fn default() -> Self {
        Self {
            deny: Vec::new(),
            ask: Vec::new(),
            allow: vec![
                "ls(*)".to_string(),
                "grep(*)".to_string(),
                "find(*)".to_string(),
                "subagent(*)".to_string(),
                "askUserQuestion(*)".to_string(),
            ],
            mode: "on-request".to_string(),
        }
    }
}

/// Global settings for the optional, read-only dev-flow integration.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DevFlowSettings {
    pub enabled: bool,
    pub show_sidebar_status: bool,
    pub executable_path: Option<PathBuf>,
}

impl Default for DevFlowSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            show_sidebar_status: true,
            executable_path: None,
        }
    }
}

/// Fully resolved settings (all fields have values)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub default_provider: Option<String>,
    pub default_model: Option<String>,
    /// Optional low-cost model used by bounded auxiliary requests.
    pub small_model: Option<String>,
    #[serde(alias = "default_thinking_level")]
    pub default_thinking_effort: Option<rozsa_model::types::ThinkingEffort>,
    pub compaction: CompactionSettings,
    pub retry: RetrySettings,
    pub transport: String,
    pub block_images: bool,
    pub hide_thinking: bool,
    /// Generate a concise session name after the first real user turn.
    pub auto_session_naming: bool,
    pub steering_mode: String,
    pub follow_up_mode: String,
    pub running_send_mode: String,
    #[serde(rename = "permission", alias = "permissions")]
    pub permissions: PermissionSettings,
    pub context_window_preferences: HashMap<String, u64>,
    pub lsp_mode: String,
    /// Per-tool enablement. Missing entries are enabled.
    #[serde(default)]
    pub tools: BTreeMap<String, bool>,
    /// Per-skill enablement. Missing entries are enabled.
    #[serde(default)]
    pub skills: BTreeMap<String, bool>,
    #[serde(default)]
    pub appearance: AppearanceSettings,
    #[serde(default)]
    pub dev_flow: DevFlowSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            default_provider: None,
            default_model: None,
            small_model: None,
            default_thinking_effort: None,
            compaction: CompactionSettings::default(),
            retry: RetrySettings::default(),
            transport: "auto".to_string(),
            block_images: false,
            hide_thinking: false,
            auto_session_naming: true,
            steering_mode: "one-at-a-time".to_string(),
            follow_up_mode: "one-at-a-time".to_string(),
            running_send_mode: "queue".to_string(),
            permissions: PermissionSettings::default(),
            context_window_preferences: HashMap::new(),
            lsp_mode: "disabled".to_string(),
            tools: BTreeMap::new(),
            skills: BTreeMap::new(),
            appearance: AppearanceSettings::default(),
            dev_flow: DevFlowSettings::default(),
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
    pub translucent_sidebar: bool,
    #[serde(default = "default_quota_visibility")]
    pub show_rate_limits: bool,
    #[serde(default = "default_quota_visibility")]
    pub show_hourly_rate_limit: bool,
    #[serde(default = "default_quota_visibility")]
    pub show_weekly_rate_limit: bool,
    pub rate_limit_display_mode: String,
    pub light_theme: String,
    pub dark_theme: String,
}

impl Default for AppearanceSettings {
    fn default() -> Self {
        Self {
            theme_mode: "system".to_string(),
            font_size: 14,
            translucent_sidebar: false,
            show_rate_limits: true,
            show_hourly_rate_limit: true,
            show_weekly_rate_limit: true,
            rate_limit_display_mode: "remained".to_string(),
            light_theme: "rozsa".to_string(),
            dark_theme: "rozsa-dark".to_string(),
        }
    }
}

fn default_quota_visibility() -> bool {
    true
}

impl AppearanceSettings {
    pub fn validate(&self) -> Result<(), String> {
        if !matches!(self.theme_mode.as_str(), "system" | "light" | "dark") {
            return Err(format!("invalid theme mode: {}", self.theme_mode));
        }
        if !(5..=30).contains(&self.font_size) {
            return Err(format!(
                "font size must be between 5 and 30: {}",
                self.font_size
            ));
        }
        if !matches!(self.rate_limit_display_mode.as_str(), "used" | "remained") {
            return Err(format!(
                "invalid rate limit display mode: {}",
                self.rate_limit_display_mode
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
    pub small_model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "default_thinking_level")]
    pub default_thinking_effort: Option<rozsa_model::types::ThinkingEffort>,
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
    pub auto_session_naming: Option<bool>,
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
    pub tools: Option<BTreeMap<String, bool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skills: Option<BTreeMap<String, bool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub appearance: Option<PartialAppearanceSettings>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dev_flow: Option<PartialDevFlowSettings>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PartialDevFlowSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub show_sidebar_status: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub executable_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PartialAppearanceSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_size: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub translucent_sidebar: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub show_rate_limits: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub show_hourly_rate_limit: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub show_weekly_rate_limit: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate_limit_display_mode: Option<String>,
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
}
