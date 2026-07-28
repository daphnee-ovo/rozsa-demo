use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

pub const BUILTIN_LIGHT_THEME_ID: &str = "rozsa";
pub const BUILTIN_DARK_THEME_ID: &str = "rozsa-dark";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    Light,
    Dark,
}

impl ThemeMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeDefinition {
    pub id: String,
    pub name: String,
    pub mode: ThemeMode,
    pub accent: String,
    pub background: String,
    pub foreground: String,
    pub ui_font: String,
    pub translucent_sidebar: bool,
    pub code_font: String,
    #[serde(default)]
    pub variables: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeSummary {
    pub id: String,
    pub name: String,
    pub mode: ThemeMode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ThemeFile {
    mode: ThemeMode,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    accent: Option<String>,
    #[serde(default)]
    background: Option<String>,
    #[serde(default)]
    foreground: Option<String>,
    #[serde(default)]
    ui_font: Option<String>,
    #[serde(default)]
    translucent_sidebar: Option<bool>,
    #[serde(default)]
    code_font: Option<String>,
    #[serde(default)]
    variables: BTreeMap<String, String>,
}

#[derive(Debug, Error)]
pub enum ThemeError {
    #[error("Failed to read theme file {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("Failed to parse theme file {path}: {source}")]
    Parse {
        path: PathBuf,
        source: serde_json::Error,
    },
    #[error("Invalid theme {id}: {message}")]
    Invalid { id: String, message: String },
    #[error("Theme {id} was not found")]
    NotFound { id: String },
}

#[derive(Debug, Clone)]
pub struct ThemeStore {
    roots: Vec<PathBuf>,
    writable_root: PathBuf,
}

impl ThemeStore {
    pub fn new(root: PathBuf) -> Self {
        Self {
            roots: vec![root.clone()],
            writable_root: root,
        }
    }

    pub fn layered(global_root: PathBuf, project_root: PathBuf) -> Self {
        Self {
            roots: vec![global_root.clone(), project_root],
            writable_root: global_root,
        }
    }

    pub fn root(&self) -> &Path {
        &self.writable_root
    }

    pub fn list(&self) -> Result<Vec<ThemeSummary>, ThemeError> {
        let mut themes = vec![
            ThemeSummary {
                id: BUILTIN_LIGHT_THEME_ID.to_string(),
                name: "Rozsa".to_string(),
                mode: ThemeMode::Light,
            },
            ThemeSummary {
                id: BUILTIN_DARK_THEME_ID.to_string(),
                name: "Rozsa Dark".to_string(),
                mode: ThemeMode::Dark,
            },
        ];

        let mut custom_by_id = BTreeMap::new();
        for root in &self.roots {
            if !root.exists() {
                continue;
            }
            let entries = fs::read_dir(root).map_err(|source| ThemeError::Read {
                path: root.clone(),
                source,
            })?;
            for entry in entries {
                let entry = entry.map_err(|source| ThemeError::Read {
                    path: root.clone(),
                    source,
                })?;
                let path = entry.path();
                if path.extension().and_then(|value| value.to_str()) != Some("json") {
                    continue;
                }
                let id = path
                    .file_stem()
                    .and_then(|value| value.to_str())
                    .ok_or_else(|| ThemeError::Invalid {
                        id: path.display().to_string(),
                        message: "theme filename must be valid UTF-8".to_string(),
                    })?
                    .to_string();
                let file = self.read_file(&path, &id)?;
                Self::validate_id(&id)?;
                custom_by_id.insert(
                    id.clone(),
                    ThemeSummary {
                        id,
                        name: file.name.unwrap_or_else(|| "Custom Theme".to_string()),
                        mode: file.mode,
                    },
                );
            }
        }
        let mut custom = custom_by_id.into_values().collect::<Vec<_>>();
        custom.sort_by(|left, right| left.name.cmp(&right.name));
        themes.extend(custom);
        Ok(themes)
    }

    pub fn load(&self, id: &str, mode: ThemeMode) -> Result<ThemeDefinition, ThemeError> {
        Self::validate_id(id)?;
        if id == BUILTIN_LIGHT_THEME_ID {
            return (mode == ThemeMode::Light)
                .then(|| Self::builtin(ThemeMode::Light))
                .ok_or_else(|| ThemeError::Invalid {
                    id: id.to_string(),
                    message: "Rozsa is a light theme".to_string(),
                });
        }
        if id == BUILTIN_DARK_THEME_ID {
            return (mode == ThemeMode::Dark)
                .then(|| Self::builtin(ThemeMode::Dark))
                .ok_or_else(|| ThemeError::Invalid {
                    id: id.to_string(),
                    message: "Rozsa Dark is a dark theme".to_string(),
                });
        }

        let path = self
            .roots
            .iter()
            .rev()
            .map(|root| root.join(format!("{id}.json")))
            .find(|path| path.exists())
            .ok_or_else(|| ThemeError::NotFound { id: id.to_string() })?;
        let file = self.read_file(&path, id)?;
        if file.mode != mode {
            return Err(ThemeError::Invalid {
                id: id.to_string(),
                message: format!(
                    "theme mode is {}, requested {}",
                    file.mode.as_str(),
                    mode.as_str()
                ),
            });
        }
        Self::from_file(id, file)
    }

    pub fn save(&self, theme: &ThemeDefinition) -> Result<(), ThemeError> {
        Self::validate_id(&theme.id)?;
        if theme.id == BUILTIN_LIGHT_THEME_ID || theme.id == BUILTIN_DARK_THEME_ID {
            return Err(ThemeError::Invalid {
                id: theme.id.clone(),
                message: "built-in themes are read-only".to_string(),
            });
        }
        Self::validate_definition(theme)?;
        fs::create_dir_all(&self.writable_root).map_err(|source| ThemeError::Read {
            path: self.writable_root.clone(),
            source,
        })?;
        let path = self.writable_root.join(format!("{}.json", theme.id));
        let json =
            serde_json::to_string_pretty(&Self::from_definition(theme)).map_err(|source| {
                ThemeError::Invalid {
                    id: theme.id.clone(),
                    message: format!("cannot serialize theme: {source}"),
                }
            })?;
        fs::write(&path, json).map_err(|source| ThemeError::Read { path, source })
    }

    fn read_file(&self, path: &Path, id: &str) -> Result<ThemeFile, ThemeError> {
        let content = fs::read_to_string(path).map_err(|source| ThemeError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        serde_json::from_str(&content)
            .map_err(|source| ThemeError::Parse {
                path: path.to_path_buf(),
                source,
            })
            .and_then(|file| {
                Self::validate_file(id, &file)?;
                Ok(file)
            })
    }

    fn from_file(id: &str, file: ThemeFile) -> Result<ThemeDefinition, ThemeError> {
        let mut theme = Self::builtin(file.mode);
        theme.id = id.to_string();
        theme.name = file.name.unwrap_or_else(|| id.to_string());
        if let Some(value) = file.accent {
            theme.accent = value;
        }
        if let Some(value) = file.background {
            theme.background = value;
        }
        if let Some(value) = file.foreground {
            theme.foreground = value;
        }
        if let Some(value) = file.ui_font {
            theme.ui_font = value;
        }
        if let Some(value) = file.translucent_sidebar {
            theme.translucent_sidebar = value;
        }
        if let Some(value) = file.code_font {
            theme.code_font = value;
        }
        theme.variables.extend(file.variables);
        Self::validate_definition(&theme)?;
        Ok(theme)
    }

    fn builtin(mode: ThemeMode) -> ThemeDefinition {
        let (id, name, accent, background, foreground, variables) = match mode {
            ThemeMode::Light => (
                BUILTIN_LIGHT_THEME_ID,
                "Rozsa",
                "#D7827E",
                "#FFFFFF",
                "#575279",
                BTreeMap::from([
                    ("--surface".to_string(), "oklch(100% 0 0)".to_string()),
                    ("--muted".to_string(), "oklch(55% 0.01 350)".to_string()),
                    ("--border".to_string(), "oklch(90% 0.006 350)".to_string()),
                    (
                        "--accent-hover".to_string(),
                        "oklch(54% 0.08 355)".to_string(),
                    ),
                    (
                        "--accent-btn".to_string(),
                        "oklch(50% 0.08 355)".to_string(),
                    ),
                    ("--accent-bg".to_string(), "oklch(96% 0.02 355)".to_string()),
                    (
                        "--accent-border".to_string(),
                        "oklch(88% 0.035 355)".to_string(),
                    ),
                    ("--success".to_string(), "oklch(48% 0.10 155)".to_string()),
                    (
                        "--success-bg".to_string(),
                        "oklch(96% 0.025 155)".to_string(),
                    ),
                    ("--error".to_string(), "oklch(52% 0.14 25)".to_string()),
                    ("--error-bg".to_string(), "oklch(96% 0.025 25)".to_string()),
                    ("--warning".to_string(), "oklch(70% 0.12 85)".to_string()),
                    ("--warning-bg".to_string(), "oklch(97% 0.03 85)".to_string()),
                    ("--user-bg".to_string(), "oklch(94% 0.015 355)".to_string()),
                    ("--code-bg".to_string(), "oklch(96% 0.003 260)".to_string()),
                    (
                        "--code-border".to_string(),
                        "oklch(90% 0.005 260)".to_string(),
                    ),
                    (
                        "--sidebar-bg".to_string(),
                        "oklch(97.5% 0.004 350)".to_string(),
                    ),
                    (
                        "--titlebar-bg".to_string(),
                        "oklch(98.5% 0.003 350)".to_string(),
                    ),
                ]),
            ),
            ThemeMode::Dark => (
                BUILTIN_DARK_THEME_ID,
                "Rozsa Dark",
                "#d88991",
                "#1d1a1c",
                "#f1e9eb",
                BTreeMap::from([
                    ("--surface".to_string(), "#282326".to_string()),
                    ("--muted".to_string(), "#b6a8ad".to_string()),
                    ("--border".to_string(), "#493f43".to_string()),
                    ("--accent-hover".to_string(), "#efabb1".to_string()),
                    ("--accent-btn".to_string(), "#c8757e".to_string()),
                    ("--accent-bg".to_string(), "#3b282d".to_string()),
                    ("--accent-border".to_string(), "#66434a".to_string()),
                    ("--success".to_string(), "#82c59a".to_string()),
                    ("--success-bg".to_string(), "#20392a".to_string()),
                    ("--error".to_string(), "#f09a9a".to_string()),
                    ("--error-bg".to_string(), "#43292b".to_string()),
                    ("--warning".to_string(), "#e5bf75".to_string()),
                    ("--warning-bg".to_string(), "#413722".to_string()),
                    ("--user-bg".to_string(), "#35262c".to_string()),
                    ("--code-bg".to_string(), "#171517".to_string()),
                    ("--code-border".to_string(), "#40383c".to_string()),
                    ("--sidebar-bg".to_string(), "#211d1f".to_string()),
                    ("--titlebar-bg".to_string(), "#181618".to_string()),
                ]),
            ),
        };

        ThemeDefinition {
            id: id.to_string(),
            name: name.to_string(),
            mode,
            accent: accent.to_string(),
            background: background.to_string(),
            foreground: foreground.to_string(),
            ui_font: "-apple-system, BlinkMacSystemFont, 'SF Pro Text', 'Segoe UI', system-ui, sans-serif".to_string(),
            translucent_sidebar: false,
            code_font: "'JetBrains Mono', 'Cascadia Code', 'Fira Code', ui-monospace, Menlo, monospace".to_string(),
            variables,
        }
    }

    fn from_definition(theme: &ThemeDefinition) -> ThemeFile {
        ThemeFile {
            mode: theme.mode,
            name: Some(theme.name.clone()),
            accent: Some(theme.accent.clone()),
            background: Some(theme.background.clone()),
            foreground: Some(theme.foreground.clone()),
            ui_font: Some(theme.ui_font.clone()),
            translucent_sidebar: Some(theme.translucent_sidebar),
            code_font: Some(theme.code_font.clone()),
            variables: theme.variables.clone(),
        }
    }

    fn validate_file(id: &str, file: &ThemeFile) -> Result<(), ThemeError> {
        Self::validate_id(id)?;
        for (key, value) in &file.variables {
            Self::validate_variable(key, value).map_err(|message| ThemeError::Invalid {
                id: id.to_string(),
                message,
            })?;
        }
        for value in [
            file.accent.as_deref(),
            file.background.as_deref(),
            file.foreground.as_deref(),
            file.ui_font.as_deref(),
            file.code_font.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            Self::validate_css_value(value).map_err(|message| ThemeError::Invalid {
                id: id.to_string(),
                message,
            })?;
        }
        Ok(())
    }

    fn validate_definition(theme: &ThemeDefinition) -> Result<(), ThemeError> {
        Self::validate_id(&theme.id)?;
        for value in [
            &theme.accent,
            &theme.background,
            &theme.foreground,
            &theme.ui_font,
            &theme.code_font,
        ] {
            Self::validate_css_value(value).map_err(|message| ThemeError::Invalid {
                id: theme.id.clone(),
                message,
            })?;
        }
        for (key, value) in &theme.variables {
            Self::validate_variable(key, value).map_err(|message| ThemeError::Invalid {
                id: theme.id.clone(),
                message,
            })?;
        }
        Ok(())
    }

    fn validate_css_value(value: &str) -> Result<(), String> {
        if value.trim().is_empty() {
            return Err("CSS values cannot be empty".to_string());
        }
        if value.len() > 512
            || value
                .chars()
                .any(|ch| matches!(ch, '\n' | '\r' | ';' | '{' | '}'))
        {
            return Err("CSS values contain unsupported characters or are too long".to_string());
        }
        Ok(())
    }

    fn validate_variable(key: &str, value: &str) -> Result<(), String> {
        if !key.starts_with("--")
            || key.len() < 3
            || !key
                .chars()
                .skip(2)
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
        {
            return Err(format!("invalid CSS variable name: {key}"));
        }
        Self::validate_css_value(value)
    }

    fn validate_id(id: &str) -> Result<(), ThemeError> {
        if id.is_empty()
            || id == "."
            || id == ".."
            || !id
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
        {
            return Err(ThemeError::Invalid {
                id: id.to_string(),
                message: "theme id must contain only letters, numbers, '-' or '_'".to_string(),
            });
        }
        Ok(())
    }
}

pub fn layered_theme_store(
    roots: &crate::config_paths::ConfigRoots,
) -> Result<ThemeStore, ThemeError> {
    let [global, project] = roots.theme_dirs();
    Ok(ThemeStore::layered(global, project))
}
