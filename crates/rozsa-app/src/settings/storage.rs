// FrameworkTree
// storage.rs
// ├── enum SettingsError
// ├── enum SettingsScope
// ├── enum CapabilityKind
// ├── enum PermissionRuleKind
// ├── struct SettingsManager
// ├── impl SettingsManager
// ├── load()
// ├── reload()
// ├── read_partial()
// ├── default_provider()
// ├── default_model()
// ├── default_thinking_level()
// ├── default_thinking_level_parsed()
// ├── compaction()
// ├── retry()
// ├── transport()
// ├── block_images()
// ├── steering_mode()
// ├── follow_up_mode()
// ├── permissions()
// ├── project_path()
// ├── context_window_preference()
// ├── context_window_preferences()
// ├── resolved()
// ├── resolved_mut()
// ├── capability_enabled()
// ├── capability_overrides()
// ├── set_capability_override()
// ├── permission_rule_overrides()
// ├── set_permission_rule_overrides()
// ├── permission_mode_override()
// ├── set_permission_mode_override()
// ├── global_path()
// ├── path_for_scope()
// ├── add_project_permission_allow()
// ├── save_global()
// ├── read_json_object_or_empty()
// ├── remove_retired_permission_fields()
// └── write_json()

use super::merge::merge_settings;
use super::schema::{PartialSettings, Settings};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SettingsError {
    #[error("Failed to read settings file {path}: {source}")]
    ReadError {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("Failed to parse settings file {path}: {source}")]
    ParseError {
        path: PathBuf,
        source: serde_json::Error,
    },
    #[error("Invalid settings: {message}")]
    Invalid { message: String },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SettingsScope {
    Global,
    Project,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CapabilityKind {
    Tools,
    Skills,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PermissionRuleKind {
    Deny,
    Ask,
    Allow,
}

/// Settings manager: loads, merges, and provides access to settings
#[derive(Clone)]
pub struct SettingsManager {
    global_path: PathBuf,
    project_path: Option<PathBuf>,
    local_path: Option<PathBuf>,
    resolved: Settings,
}

impl SettingsManager {
    /// Load settings from files and merge: default -> global -> project -> local
    pub fn load(
        global_path: PathBuf,
        project_path: Option<PathBuf>,
        local_path: Option<PathBuf>,
    ) -> Result<Self, SettingsError> {
        let mut resolved = Settings::default();

        // Merge global settings
        if global_path.exists() {
            let partial = Self::read_partial(&global_path)?;
            resolved = merge_settings(&resolved, &partial);
        }

        // Merge project settings
        if let Some(ref path) = project_path {
            if path.exists() {
                let partial = Self::read_partial(path)?;
                resolved = merge_settings(&resolved, &partial);
            }
        }

        // Merge local settings
        if let Some(ref path) = local_path {
            if path.exists() {
                let partial = Self::read_partial(path)?;
                resolved = merge_settings(&resolved, &partial);
            }
        }

        resolved
            .appearance
            .validate()
            .map_err(|message| SettingsError::Invalid { message })?;

        Ok(Self {
            global_path,
            project_path,
            local_path,
            resolved,
        })
    }

    /// Reload settings from disk and re-merge
    pub fn reload(&mut self) -> Result<(), SettingsError> {
        let mut resolved = Settings::default();

        if self.global_path.exists() {
            let partial = Self::read_partial(&self.global_path)?;
            resolved = merge_settings(&resolved, &partial);
        }

        if let Some(ref path) = self.project_path {
            if path.exists() {
                let partial = Self::read_partial(path)?;
                resolved = merge_settings(&resolved, &partial);
            }
        }

        if let Some(ref path) = self.local_path {
            if path.exists() {
                let partial = Self::read_partial(path)?;
                resolved = merge_settings(&resolved, &partial);
            }
        }

        self.resolved = resolved;
        self.resolved
            .appearance
            .validate()
            .map_err(|message| SettingsError::Invalid { message })?;
        Ok(())
    }

    /// Read and parse a partial settings file
    fn read_partial(path: &Path) -> Result<PartialSettings, SettingsError> {
        let content = fs::read_to_string(path).map_err(|source| SettingsError::ReadError {
            path: path.to_path_buf(),
            source,
        })?;
        serde_json::from_str(&content).map_err(|source| SettingsError::ParseError {
            path: path.to_path_buf(),
            source,
        })
    }

    // Getter methods
    pub fn default_provider(&self) -> Option<&str> {
        self.resolved.default_provider.as_deref()
    }

    pub fn default_model(&self) -> Option<&str> {
        self.resolved.default_model.as_deref()
    }

    pub fn default_thinking_level(&self) -> rozsa_model::types::ThinkingLevel {
        self.resolved
            .default_thinking_level
            .unwrap_or(rozsa_model::types::ThinkingLevel::Off)
    }

    pub fn default_thinking_level_parsed(&self) -> rozsa_model::types::ThinkingLevel {
        self.default_thinking_level()
    }

    pub fn compaction(&self) -> &super::schema::CompactionSettings {
        &self.resolved.compaction
    }

    pub fn retry(&self) -> &super::schema::RetrySettings {
        &self.resolved.retry
    }

    pub fn transport(&self) -> &str {
        &self.resolved.transport
    }

    pub fn block_images(&self) -> bool {
        self.resolved.block_images
    }

    pub fn steering_mode(&self) -> &str {
        &self.resolved.steering_mode
    }

    pub fn follow_up_mode(&self) -> &str {
        &self.resolved.follow_up_mode
    }

    pub fn permissions(&self) -> &super::schema::PermissionSettings {
        &self.resolved.permissions
    }

    pub fn project_path(&self) -> Option<&Path> {
        self.project_path.as_deref()
    }

    pub fn context_window_preference(&self, key: &str) -> Option<u64> {
        self.resolved.context_window_preferences.get(key).copied()
    }

    pub fn context_window_preferences(&self) -> &std::collections::HashMap<String, u64> {
        &self.resolved.context_window_preferences
    }

    /// Get the fully resolved settings
    pub fn resolved(&self) -> &Settings {
        &self.resolved
    }

    /// Mutable access to resolved settings for runtime changes.
    pub fn resolved_mut(&mut self) -> &mut Settings {
        &mut self.resolved
    }

    pub fn capability_enabled(&self, kind: CapabilityKind, name: &str) -> bool {
        let configured = match kind {
            CapabilityKind::Tools => &self.resolved.tools,
            CapabilityKind::Skills => &self.resolved.skills,
        };
        configured.get(name).copied().unwrap_or(true)
    }

    pub fn capability_overrides(
        &self,
        scope: SettingsScope,
        kind: CapabilityKind,
    ) -> Result<BTreeMap<String, bool>, SettingsError> {
        let path = self.path_for_scope(scope)?;
        if !path.exists() {
            return Ok(BTreeMap::new());
        }
        let partial = Self::read_partial(path)?;
        Ok(match kind {
            CapabilityKind::Tools => partial.tools.unwrap_or_default(),
            CapabilityKind::Skills => partial.skills.unwrap_or_default(),
        })
    }

    /// Set or remove one layer-local capability override while preserving every
    /// unrelated settings key in the file, then refresh the resolved overlay.
    pub fn set_capability_override(
        &mut self,
        scope: SettingsScope,
        kind: CapabilityKind,
        name: &str,
        enabled: Option<bool>,
    ) -> Result<(), SettingsError> {
        if name.trim().is_empty() {
            return Err(SettingsError::Invalid {
                message: "capability name cannot be empty".to_owned(),
            });
        }
        let path = self.path_for_scope(scope)?.to_path_buf();
        let mut value = read_json_object_or_empty(&path)?;
        let field = match kind {
            CapabilityKind::Tools => "tools",
            CapabilityKind::Skills => "skills",
        };
        let root = value
            .as_object_mut()
            .ok_or_else(|| SettingsError::Invalid {
                message: format!("settings root must be an object: {}", path.display()),
            })?;
        let capabilities = root
            .entry(field.to_owned())
            .or_insert_with(|| serde_json::json!({}));
        let capabilities = capabilities
            .as_object_mut()
            .ok_or_else(|| SettingsError::Invalid {
                message: format!("settings.{field} must be an object"),
            })?;
        match enabled {
            Some(enabled) => {
                capabilities.insert(name.to_owned(), serde_json::Value::Bool(enabled));
            }
            None => {
                capabilities.remove(name);
            }
        }
        if capabilities.is_empty() {
            root.remove(field);
        }
        write_json(&path, &value)?;
        self.reload()
    }

    pub fn permission_rule_overrides(
        &self,
        scope: SettingsScope,
        kind: PermissionRuleKind,
    ) -> Result<Option<Vec<String>>, SettingsError> {
        let path = self.path_for_scope(scope)?;
        if !path.exists() {
            return Ok(None);
        }
        let partial = Self::read_partial(path)?;
        let Some(permission) = partial.permissions else {
            return Ok(None);
        };
        Ok(match kind {
            PermissionRuleKind::Deny => permission.deny,
            PermissionRuleKind::Ask => permission.ask,
            PermissionRuleKind::Allow => permission.allow,
        })
    }

    pub fn set_permission_rule_overrides(
        &mut self,
        scope: SettingsScope,
        kind: PermissionRuleKind,
        rules: Option<Vec<String>>,
    ) -> Result<(), SettingsError> {
        let path = self.path_for_scope(scope)?.to_path_buf();
        let mut value = read_json_object_or_empty(&path)?;
        let root = value
            .as_object_mut()
            .ok_or_else(|| SettingsError::Invalid {
                message: format!("settings root must be an object: {}", path.display()),
            })?;
        let permission = root
            .entry("permission".to_owned())
            .or_insert_with(|| serde_json::json!({}));
        let permission = permission
            .as_object_mut()
            .ok_or_else(|| SettingsError::Invalid {
                message: "settings.permission must be an object".to_owned(),
            })?;
        remove_retired_permission_fields(permission);
        let field = match kind {
            PermissionRuleKind::Deny => "deny",
            PermissionRuleKind::Ask => "ask",
            PermissionRuleKind::Allow => "allow",
        };
        match rules {
            Some(rules) => {
                permission.insert(
                    field.to_owned(),
                    serde_json::Value::Array(
                        rules.into_iter().map(serde_json::Value::String).collect(),
                    ),
                );
            }
            None => {
                permission.remove(field);
            }
        }
        if permission.is_empty() {
            root.remove("permission");
        }
        write_json(&path, &value)?;
        self.reload()
    }

    pub fn permission_mode_override(
        &self,
        scope: SettingsScope,
    ) -> Result<Option<String>, SettingsError> {
        let path = self.path_for_scope(scope)?;
        if !path.exists() {
            return Ok(None);
        }
        Ok(Self::read_partial(path)?
            .permissions
            .and_then(|permission| permission.mode))
    }

    pub fn set_permission_mode_override(
        &mut self,
        scope: SettingsScope,
        mode: Option<String>,
    ) -> Result<(), SettingsError> {
        if mode.as_deref() == Some("auto-approve") {
            return Err(SettingsError::Invalid {
                message:
                    "auto-approve is not implemented yet; the current permission mode was not changed"
                        .to_owned(),
            });
        }
        if mode
            .as_deref()
            .is_some_and(|mode| !matches!(mode, "on-request" | "yolo"))
        {
            return Err(SettingsError::Invalid {
                message: format!("unsupported permission mode: {}", mode.unwrap()),
            });
        }
        let path = self.path_for_scope(scope)?.to_path_buf();
        let mut value = read_json_object_or_empty(&path)?;
        let root = value
            .as_object_mut()
            .ok_or_else(|| SettingsError::Invalid {
                message: format!("settings root must be an object: {}", path.display()),
            })?;
        let permission = root
            .entry("permission".to_owned())
            .or_insert_with(|| serde_json::json!({}));
        let permission = permission
            .as_object_mut()
            .ok_or_else(|| SettingsError::Invalid {
                message: "settings.permission must be an object".to_owned(),
            })?;
        remove_retired_permission_fields(permission);
        match mode {
            Some(mode) => {
                permission.insert("mode".to_owned(), serde_json::Value::String(mode));
            }
            None => {
                permission.remove("mode");
            }
        }
        if permission.is_empty() {
            root.remove("permission");
        }
        write_json(&path, &value)?;
        self.reload()
    }

    pub fn global_path(&self) -> &Path {
        &self.global_path
    }

    fn path_for_scope(&self, scope: SettingsScope) -> Result<&Path, SettingsError> {
        match scope {
            SettingsScope::Global => Ok(&self.global_path),
            SettingsScope::Project => {
                self.project_path
                    .as_deref()
                    .ok_or_else(|| SettingsError::Invalid {
                        message: "project settings path is unavailable".to_owned(),
                    })
            }
        }
    }

    /// Append an automatically granted project trust without ever modifying the
    /// user-level settings file. Global permission rules are manual-only.
    pub fn add_project_permission_allow(&self, rule: &str) -> Result<(), SettingsError> {
        let Some(path) = self.project_path.as_ref() else {
            return Ok(());
        };
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| SettingsError::ReadError {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        let mut value = if path.exists() {
            let content = fs::read_to_string(path).map_err(|source| SettingsError::ReadError {
                path: path.clone(),
                source,
            })?;
            serde_json::from_str::<serde_json::Value>(&content).map_err(|source| {
                SettingsError::ParseError {
                    path: path.clone(),
                    source,
                }
            })?
        } else {
            serde_json::json!({})
        };
        let root = value
            .as_object_mut()
            .expect("settings JSON root must be object");
        let permission = root
            .entry("permission".to_string())
            .or_insert_with(|| serde_json::json!({}));
        let permission = permission
            .as_object_mut()
            .expect("permission settings must be an object");
        let allow = permission
            .entry("allow".to_string())
            .or_insert_with(|| serde_json::json!([]));
        let allow = allow
            .as_array_mut()
            .expect("permission.allow must be an array");
        if !allow.iter().any(|value| value.as_str() == Some(rule)) {
            allow.push(serde_json::Value::String(rule.to_string()));
            let json = serde_json::to_string_pretty(&value).map_err(|source| {
                SettingsError::ParseError {
                    path: path.clone(),
                    source,
                }
            })?;
            fs::write(path, json).map_err(|source| SettingsError::ReadError {
                path: path.clone(),
                source,
            })?;
        }
        Ok(())
    }

    /// Persist current resolved settings to the global settings file.
    pub fn save_global(&self) -> Result<(), SettingsError> {
        if let Some(parent) = self.global_path.parent() {
            fs::create_dir_all(parent).map_err(|source| SettingsError::ReadError {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        let json = serde_json::to_string_pretty(&self.resolved).map_err(|source| {
            SettingsError::ParseError {
                path: self.global_path.clone(),
                source,
            }
        })?;
        fs::write(&self.global_path, json).map_err(|source| SettingsError::ReadError {
            path: self.global_path.clone(),
            source,
        })
    }
}

fn read_json_object_or_empty(path: &Path) -> Result<serde_json::Value, SettingsError> {
    if !path.exists() {
        return Ok(serde_json::json!({}));
    }
    let content = fs::read_to_string(path).map_err(|source| SettingsError::ReadError {
        path: path.to_path_buf(),
        source,
    })?;
    let value = serde_json::from_str::<serde_json::Value>(&content).map_err(|source| {
        SettingsError::ParseError {
            path: path.to_path_buf(),
            source,
        }
    })?;
    if !value.is_object() {
        return Err(SettingsError::Invalid {
            message: format!("settings root must be an object: {}", path.display()),
        });
    }
    Ok(value)
}

fn remove_retired_permission_fields(permission: &mut serde_json::Map<String, serde_json::Value>) {
    permission.remove("allowed_tools");
    permission.remove("blocked_commands");
    permission.remove("auto_approve_patterns");
}

fn write_json(path: &Path, value: &serde_json::Value) -> Result<(), SettingsError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| SettingsError::ReadError {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    let json = serde_json::to_string_pretty(value).map_err(|source| SettingsError::ParseError {
        path: path.to_path_buf(),
        source,
    })?;
    fs::write(path, format!("{json}\n")).map_err(|source| SettingsError::ReadError {
        path: path.to_path_buf(),
        source,
    })
}
