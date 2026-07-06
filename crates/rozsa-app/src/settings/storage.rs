use super::merge::merge_settings;
use super::schema::{PartialSettings, Settings};
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

    /// Add a trust_key as an auto-approve pattern and persist to settings.
    /// Converts the trust_key into a regex pattern (exact prefix match).
    pub fn add_trusted_pattern(&mut self, trust_key: &str) {
        let pattern = format!("^{}", regex::escape(trust_key));
        if !self
            .resolved
            .permissions
            .auto_approve_patterns
            .contains(&pattern)
        {
            self.resolved
                .permissions
                .auto_approve_patterns
                .push(pattern);
            let _ = self.save_global();
        }
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
