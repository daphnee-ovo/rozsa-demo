// FrameworkTree
// key_bindings.rs
// ├── enum KeyBindingAction
// ├── impl KeyBindingAction
// ├── title()
// ├── description()
// ├── default_binding()
// ├── scope()
// ├── enum KeyBindingScope
// ├── struct KeyBindingDefinition
// ├── struct KeyBindingFile
// ├── key_bindings_path()
// ├── load_key_bindings()
// ├── update_key_binding()
// ├── reset_key_binding()
// ├── overrides_from_definitions()
// ├── write_key_binding_file()
// ├── normalized_collision_key()
// └── validate_binding()

//! GUI-owned keyboard shortcut registry and persistence.
//!
//! The file stores only user overrides. Loading always overlays those values on
//! the typed defaults below, so adding a future action does not require users
//! to rewrite their override file.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use rozsa_app::config_paths::ConfigRoots;
use serde::{Deserialize, Serialize};

const KEY_BINDINGS_FILE: &str = "key_bindings.json";
const FILE_VERSION: u8 = 1;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum KeyBindingAction {
    ToggleThinking,
    OpenModelPicker,
    NewSession,
    OpenSettings,
    SendMessage,
    InsertNewline,
    FocusComposer,
}

impl KeyBindingAction {
    pub const ALL: [Self; 7] = [
        Self::ToggleThinking,
        Self::OpenModelPicker,
        Self::NewSession,
        Self::OpenSettings,
        Self::SendMessage,
        Self::InsertNewline,
        Self::FocusComposer,
    ];

    fn title(self) -> &'static str {
        match self {
            Self::ToggleThinking => "Toggle thinking",
            Self::OpenModelPicker => "Choose model",
            Self::NewSession => "New session",
            Self::OpenSettings => "Open settings",
            Self::SendMessage => "Send message",
            Self::InsertNewline => "Insert new line",
            Self::FocusComposer => "Focus composer",
        }
    }

    fn description(self) -> &'static str {
        match self {
            Self::ToggleThinking => "Expand or collapse thinking blocks",
            Self::OpenModelPicker => "Open the model picker",
            Self::NewSession => "Start a new session",
            Self::OpenSettings => "Open or close Settings",
            Self::SendMessage => "Send from the focused composer",
            Self::InsertNewline => "Add a line in the focused composer",
            Self::FocusComposer => "Move focus to the message composer",
        }
    }

    fn default_binding(self) -> &'static str {
        match self {
            Self::ToggleThinking => "Ctrl+T",
            Self::OpenModelPicker => "Ctrl+P",
            Self::NewSession => "Ctrl+N",
            Self::OpenSettings => "Ctrl+,",
            Self::SendMessage => "Enter",
            Self::InsertNewline => "Shift+Enter",
            Self::FocusComposer => "/",
        }
    }

    fn scope(self) -> KeyBindingScope {
        match self {
            Self::SendMessage | Self::InsertNewline => KeyBindingScope::Composer,
            Self::FocusComposer => KeyBindingScope::OutsideComposer,
            _ => KeyBindingScope::Global,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum KeyBindingScope {
    Global,
    Composer,
    OutsideComposer,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyBindingDefinition {
    pub action: KeyBindingAction,
    pub title: &'static str,
    pub description: &'static str,
    pub default_binding: &'static str,
    pub binding: String,
    pub scope: KeyBindingScope,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct KeyBindingFile {
    version: u8,
    #[serde(default)]
    bindings: BTreeMap<KeyBindingAction, String>,
}

pub fn key_bindings_path(config_roots: &ConfigRoots) -> PathBuf {
    config_roots.global().join(KEY_BINDINGS_FILE)
}

pub fn load_key_bindings(path: &Path) -> Result<Vec<KeyBindingDefinition>, String> {
    let overrides = if path.exists() {
        let text = fs::read_to_string(path)
            .map_err(|error| format!("Failed to read {}: {error}", path.display()))?;
        let file: KeyBindingFile = serde_json::from_str(&text)
            .map_err(|error| format!("Invalid {}: {error}", path.display()))?;
        if file.version != FILE_VERSION {
            return Err(format!(
                "Unsupported keyboard shortcut file version {} in {}",
                file.version,
                path.display()
            ));
        }
        file.bindings
    } else {
        BTreeMap::new()
    };

    let mut definitions = Vec::with_capacity(KeyBindingAction::ALL.len());
    let mut occupied = BTreeSet::new();
    for action in KeyBindingAction::ALL {
        let binding = overrides
            .get(&action)
            .cloned()
            .unwrap_or_else(|| action.default_binding().to_owned());
        validate_binding(&binding)?;
        let collision_key = normalized_collision_key(&binding);
        if !occupied.insert(collision_key) {
            return Err(format!("Duplicate keyboard shortcut: {binding}"));
        }
        definitions.push(KeyBindingDefinition {
            action,
            title: action.title(),
            description: action.description(),
            default_binding: action.default_binding(),
            binding,
            scope: action.scope(),
        });
    }
    Ok(definitions)
}

pub fn update_key_binding(
    path: &Path,
    action: KeyBindingAction,
    binding: &str,
) -> Result<Vec<KeyBindingDefinition>, String> {
    validate_binding(binding)?;
    let current = load_key_bindings(path)?;
    if let Some(conflict) = current.iter().find(|definition| {
        definition.action != action
            && normalized_collision_key(&definition.binding) == normalized_collision_key(binding)
    }) {
        return Err(format!(
            "{binding} is already assigned to {}",
            conflict.title
        ));
    }

    let mut overrides = overrides_from_definitions(&current);
    if binding == action.default_binding() {
        overrides.remove(&action);
    } else {
        overrides.insert(action, binding.to_owned());
    }
    write_key_binding_file(path, overrides)?;
    load_key_bindings(path)
}

pub fn reset_key_binding(
    path: &Path,
    action: KeyBindingAction,
) -> Result<Vec<KeyBindingDefinition>, String> {
    let current = load_key_bindings(path)?;
    let mut overrides = overrides_from_definitions(&current);
    overrides.remove(&action);
    write_key_binding_file(path, overrides)?;
    load_key_bindings(path)
}

fn overrides_from_definitions(
    definitions: &[KeyBindingDefinition],
) -> BTreeMap<KeyBindingAction, String> {
    definitions
        .iter()
        .filter(|definition| definition.binding != definition.default_binding)
        .map(|definition| (definition.action, definition.binding.clone()))
        .collect()
}

fn write_key_binding_file(
    path: &Path,
    bindings: BTreeMap<KeyBindingAction, String>,
) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("Keyboard shortcut path has no parent: {}", path.display()))?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("Failed to create {}: {error}", parent.display()))?;
    let json = serde_json::to_string_pretty(&KeyBindingFile {
        version: FILE_VERSION,
        bindings,
    })
    .map_err(|error| format!("Failed to serialize keyboard shortcuts: {error}"))?;
    fs::write(path, format!("{json}\n"))
        .map_err(|error| format!("Failed to write {}: {error}", path.display()))
}

fn normalized_collision_key(binding: &str) -> String {
    binding.to_ascii_lowercase()
}

fn validate_binding(binding: &str) -> Result<(), String> {
    if binding.trim() != binding || binding.is_empty() {
        return Err("Keyboard shortcut cannot be empty or padded with spaces".to_owned());
    }
    let parts: Vec<_> = binding.split('+').collect();
    let key = parts
        .last()
        .copied()
        .ok_or_else(|| "Keyboard shortcut must include a key".to_owned())?;
    if key.is_empty() || matches!(key, "Ctrl" | "Meta" | "Alt" | "Shift") {
        return Err("Keyboard shortcut must end with a non-modifier key".to_owned());
    }
    let modifiers = &parts[..parts.len() - 1];
    let mut seen = BTreeSet::new();
    for modifier in modifiers {
        if !matches!(*modifier, "Ctrl" | "Meta" | "Alt" | "Shift") {
            return Err(format!(
                "Unsupported keyboard shortcut modifier: {modifier}"
            ));
        }
        if !seen.insert(*modifier) {
            return Err(format!("Duplicate keyboard shortcut modifier: {modifier}"));
        }
    }
    if key.chars().count() > 1
        && !matches!(
            key,
            "Enter"
                | "Escape"
                | "Tab"
                | "Space"
                | "ArrowUp"
                | "ArrowDown"
                | "ArrowLeft"
                | "ArrowRight"
                | "Backspace"
                | "Delete"
        )
        && !key.starts_with('F')
    {
        return Err(format!("Unsupported keyboard shortcut key: {key}"));
    }
    Ok(())
}
