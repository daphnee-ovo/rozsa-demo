// File: runtime_state.rs
//
// Internal Framework:
// runtime_state.rs
// ├── EditMode                 # normal / think_first, with cycle()
// ├── ToolCallStats            # per-tool call/error counters
// ├── RuntimeState             # mutable session state
// └── RuntimeStateSnapshot     # serializable UI snapshot
//
// Related Docs:
// - [Gap Audit](../../docs/NATIVE_TUI_GAP_AUDIT.md)

use std::collections::HashMap;
use std::fmt;

use serde::{Deserialize, Serialize};

/// Agent editing mode — controls whether the model thinks before acting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EditMode {
    Normal,
    ThinkFirst,
}

impl EditMode {
    /// Cycle to the next mode.
    pub fn cycle(self) -> Self {
        match self {
            Self::Normal => Self::ThinkFirst,
            Self::ThinkFirst => Self::Normal,
        }
    }
}

impl fmt::Display for EditMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Normal => write!(f, "normal"),
            Self::ThinkFirst => write!(f, "think_first"),
        }
    }
}

/// Per-tool invocation statistics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolCallStats {
    pub tool_name: String,
    pub call_count: u64,
    pub error_count: u64,
}

/// Mutable runtime state held during an agent session.
/// Not serializable directly — use [`RuntimeState::snapshot`] for UI consumption.
#[derive(Debug, Clone)]
pub struct RuntimeState {
    pub edit_mode: EditMode,
    pub permission_mode: String,
    pub tool_stats: HashMap<String, ToolCallStats>,
}

impl RuntimeState {
    /// Create a new runtime state with the given permission mode.
    pub fn new(permission_mode: &str) -> Self {
        Self {
            edit_mode: EditMode::Normal,
            permission_mode: permission_mode.to_owned(),
            tool_stats: HashMap::new(),
        }
    }

    /// Record a tool invocation, tracking call and error counts.
    pub fn record_tool_call(&mut self, tool_name: &str, is_error: bool) {
        let stats = self.tool_stats.entry(tool_name.to_owned()).or_insert_with(|| {
            ToolCallStats {
                tool_name: tool_name.to_owned(),
                ..Default::default()
            }
        });
        stats.call_count += 1;
        if is_error {
            stats.error_count += 1;
        }
    }

    /// Produce a serializable snapshot for the TUI layer.
    pub fn snapshot(&self) -> RuntimeStateSnapshot {
        RuntimeStateSnapshot {
            edit_mode: self.edit_mode,
            permission_mode: self.permission_mode.clone(),
            tool_stats: self.tool_stats.values().cloned().collect(),
        }
    }
}

/// Frozen, serializable view of runtime state for the UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeStateSnapshot {
    pub edit_mode: EditMode,
    pub permission_mode: String,
    pub tool_stats: Vec<ToolCallStats>,
}
