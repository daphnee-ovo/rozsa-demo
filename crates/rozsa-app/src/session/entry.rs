//! Session entry types for tree-structured conversation storage.
//!
//! Session structure:
//! - SessionHeader (file header with session metadata)
//! - SessionEntry enum (tree nodes with id/parentId)
//!
//! Each entry has:
//! - id: unique short ID for this entry
//! - parentId: reference to parent entry (null for root)
//! - timestamp: ISO 8601 creation time
//!
//! Entry types:
//! - Message: user/assistant/tool-result messages
//! - Compaction: summary of removed history
//! - ModelChange: model switch record
//! - ThinkingLevelChange: reasoning level change
//! - Custom: extension-specific data (not sent to LLM)
//! - CustomMessage: extension messages (sent to LLM)
//! - Label: user bookmark/marker
//! - BranchSummary: summary of abandoned branch
//! - SessionInfo: session metadata (e.g., display name)

use serde::{Deserialize, Serialize};

/// Session header stored as the first line of a session file.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionHeader {
    #[serde(rename = "type")]
    pub entry_type: String, // Always "session"
    pub version: Option<u32>,
    pub id: String,
    pub timestamp: String,
    pub cwd: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_session: Option<String>,
}

impl SessionHeader {
    pub fn new(id: String, timestamp: String, cwd: String, parent_session: Option<String>) -> Self {
        Self {
            entry_type: "session".to_string(),
            version: Some(3), // CURRENT_SESSION_VERSION = 3
            id,
            timestamp,
            cwd,
            parent_session,
        }
    }
}

/// Base fields common to all session entries.
pub trait SessionEntryBase {
    fn id(&self) -> &str;
    fn parent_id(&self) -> Option<&str>;
    fn timestamp(&self) -> &str;
}

/// Session entry - tree node in the conversation structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SessionEntry {
    /// User, assistant, or tool-result message.
    Message {
        id: String,
        #[serde(rename = "parentId")]
        parent_id: Option<String>,
        timestamp: String,
        message: serde_json::Value, // AgentMessage (Message union + custom extensions)
    },

    /// Compaction summary entry.
    Compaction {
        id: String,
        #[serde(rename = "parentId")]
        parent_id: Option<String>,
        timestamp: String,
        summary: String,
        #[serde(rename = "firstKeptEntryId")]
        first_kept_entry_id: String,
        #[serde(rename = "tokensBefore")]
        tokens_before: u64,
        #[serde(skip_serializing_if = "Option::is_none")]
        details: Option<serde_json::Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        from_hook: Option<bool>,
    },

    /// Model change record.
    ModelChange {
        id: String,
        #[serde(rename = "parentId")]
        parent_id: Option<String>,
        timestamp: String,
        provider: String,
        #[serde(rename = "modelId")]
        model_id: String,
    },

    /// Thinking/reasoning level change.
    ThinkingLevelChange {
        id: String,
        #[serde(rename = "parentId")]
        parent_id: Option<String>,
        timestamp: String,
        #[serde(rename = "thinkingLevel")]
        thinking_level: String,
    },

    /// Custom entry for extensions (not sent to LLM).
    Custom {
        id: String,
        #[serde(rename = "parentId")]
        parent_id: Option<String>,
        timestamp: String,
        #[serde(rename = "customType")]
        custom_type: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        data: Option<serde_json::Value>,
    },

    /// Custom message entry for extensions (sent to LLM).
    CustomMessage {
        id: String,
        #[serde(rename = "parentId")]
        parent_id: Option<String>,
        timestamp: String,
        #[serde(rename = "customType")]
        custom_type: String,
        content: ContentValue,
        #[serde(skip_serializing_if = "Option::is_none")]
        details: Option<serde_json::Value>,
        display: bool,
    },

    /// Label entry for user bookmarks.
    Label {
        id: String,
        #[serde(rename = "parentId")]
        parent_id: Option<String>,
        timestamp: String,
        #[serde(rename = "targetId")]
        target_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        label: Option<String>,
    },

    /// Branch summary entry for abandoned conversation paths.
    BranchSummary {
        id: String,
        #[serde(rename = "parentId")]
        parent_id: Option<String>,
        timestamp: String,
        #[serde(rename = "fromId")]
        from_id: String,
        summary: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        details: Option<serde_json::Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        from_hook: Option<bool>,
    },

    /// Session info entry (e.g., user-defined display name).
    SessionInfo {
        id: String,
        #[serde(rename = "parentId")]
        parent_id: Option<String>,
        timestamp: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
    },
}

/// Content value for CustomMessage - can be string or array of content blocks.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ContentValue {
    String(String),
    Blocks(Vec<serde_json::Value>), // TextContent | ImageContent
}

impl SessionEntry {
    pub fn id(&self) -> &str {
        match self {
            Self::Message { id, .. }
            | Self::Compaction { id, .. }
            | Self::ModelChange { id, .. }
            | Self::ThinkingLevelChange { id, .. }
            | Self::Custom { id, .. }
            | Self::CustomMessage { id, .. }
            | Self::Label { id, .. }
            | Self::BranchSummary { id, .. }
            | Self::SessionInfo { id, .. } => id,
        }
    }

    pub fn parent_id(&self) -> Option<&str> {
        match self {
            Self::Message { parent_id, .. }
            | Self::Compaction { parent_id, .. }
            | Self::ModelChange { parent_id, .. }
            | Self::ThinkingLevelChange { parent_id, .. }
            | Self::Custom { parent_id, .. }
            | Self::CustomMessage { parent_id, .. }
            | Self::Label { parent_id, .. }
            | Self::BranchSummary { parent_id, .. }
            | Self::SessionInfo { parent_id, .. } => parent_id.as_deref(),
        }
    }

    pub fn timestamp(&self) -> &str {
        match self {
            Self::Message { timestamp, .. }
            | Self::Compaction { timestamp, .. }
            | Self::ModelChange { timestamp, .. }
            | Self::ThinkingLevelChange { timestamp, .. }
            | Self::Custom { timestamp, .. }
            | Self::CustomMessage { timestamp, .. }
            | Self::Label { timestamp, .. }
            | Self::BranchSummary { timestamp, .. }
            | Self::SessionInfo { timestamp, .. } => timestamp,
        }
    }
}

/// File entry - union of header and session entries.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FileEntry {
    Header(SessionHeader),
    Entry(SessionEntry),
}

impl FileEntry {
    pub fn is_header(&self) -> bool {
        matches!(self, Self::Header(_))
    }

    pub fn as_header(&self) -> Option<&SessionHeader> {
        match self {
            Self::Header(h) => Some(h),
            _ => None,
        }
    }

    pub fn as_entry(&self) -> Option<&SessionEntry> {
        match self {
            Self::Entry(e) => Some(e),
            _ => None,
        }
    }
}
