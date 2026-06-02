use std::{
    collections::BTreeMap,
    error::Error,
    io::Write,
    os::unix::net::UnixStream,
    sync::{Arc, Mutex},
};

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Deserialize, Default)]
#[allow(dead_code)]
pub struct NativeUiState {
    #[serde(rename = "appName")]
    pub app_name: String,
    pub version: String,
    pub cwd: String,
    #[serde(rename = "sessionName")]
    pub session_name: Option<String>,
    pub model: Option<ModelInfo>,
    #[serde(rename = "thinkingLevel")]
    pub thinking_level: String,
    #[serde(rename = "isStreaming")]
    pub is_streaming: bool,
    #[serde(rename = "isCompacting", default)]
    pub is_compacting: bool,
    #[serde(rename = "hideThinking", default)]
    pub hide_thinking: bool,
    #[serde(rename = "showImages", default = "default_true")]
    pub show_images: bool,
    pub messages: Vec<Value>,
    #[serde(rename = "pendingMessages")]
    pub pending_messages: Vec<String>,
    pub status: BTreeMap<String, String>,
    #[serde(rename = "widgetsAbove")]
    pub widgets_above: BTreeMap<String, Vec<String>>,
    #[serde(rename = "widgetsBelow")]
    pub widgets_below: BTreeMap<String, Vec<String>>,
    pub stats: Option<Value>,
    #[serde(rename = "runtimeState")]
    pub runtime_state: Option<Value>,
    #[serde(rename = "contextUsage")]
    pub context_usage: Option<Value>,
    pub keybindings: BTreeMap<String, Vec<String>>,
    pub error: Option<String>,
}

fn default_true() -> bool {
    true
}

#[derive(Clone, Debug, Deserialize)]
#[allow(dead_code)]
pub struct ModelInfo {
    pub id: String,
    pub provider: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "type")]
#[allow(clippy::large_enum_variant)]
pub enum HostMessage {
    #[serde(rename = "state")]
    State { state: NativeUiState },
    #[serde(rename = "dialog")]
    Dialog {
        id: String,
        kind: String,
        title: String,
        message: Option<String>,
        options: Option<Vec<String>>,
        text: Option<String>,
        selected: Option<usize>,
    },
    #[serde(rename = "notify")]
    Notify { level: String, message: String },
    #[serde(rename = "set_title")]
    SetTitle { title: String },
    #[serde(rename = "set_input")]
    SetInput { text: String },
    #[serde(rename = "autocomplete")]
    Autocomplete {
        #[allow(dead_code)]
        id: u64,
        prefix: String,
        items: Vec<NativeAutocompleteItem>,
    },
    #[serde(rename = "permission")]
    Permission { prompt: NativePermissionPrompt },
    #[serde(rename = "graph")]
    Graph { nodes: Vec<NativeGraphNode> },
    #[serde(rename = "sessions")]
    Sessions {
        entries: Vec<crate::components::session_selector::SessionEntry>,
        #[serde(rename = "currentSessionPath", default)]
        current_session_path: String,
    },
    #[serde(rename = "session_deleted")]
    SessionDeleted {
        path: String,
        method: String,
        error: Option<String>,
    },
    #[serde(rename = "models")]
    Models {
        entries: Vec<crate::components::model_selector::ModelEntry>,
    },
    #[serde(rename = "retry")]
    Retry { seconds: u32, reason: String },
    #[serde(rename = "compacting")]
    Compacting { active: bool },
    #[serde(rename = "shutdown")]
    Shutdown,
}

#[derive(Clone, Debug, Deserialize)]
pub struct NativePermissionPrompt {
    pub id: String,
    pub request: Value,
    pub context: Value,
    #[serde(rename = "trustLevels")]
    pub trust_levels: Vec<NativeTrustLevel>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct NativeTrustLevel {
    pub label: String,
    pub key: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct NativeGraphNode {
    pub role: String,
    pub summary: String,
    #[serde(rename = "fullText")]
    pub full_text: String,
    pub timestamp: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct NativeAutocompleteItem {
    pub value: String,
    pub label: String,
    pub description: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ImagePayload {
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub data: String,
    #[serde(rename = "mimeType")]
    pub mime_type: &'static str,
}

impl ImagePayload {
    pub fn from_base64(data: String) -> Self {
        let mime = if data.starts_with("iVBOR") {
            "image/png"
        } else if data.starts_with("/9j/") {
            "image/jpeg"
        } else if data.starts_with("R0lGOD") {
            "image/gif"
        } else if data.starts_with("UklGR") {
            "image/webp"
        } else {
            "image/png"
        };
        Self {
            kind: "image",
            data,
            mime_type: mime,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
pub enum ClientMessage<'a> {
    #[serde(rename = "submit")]
    Submit {
        text: &'a str,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        images: Vec<ImagePayload>,
    },
    #[serde(rename = "abort")]
    Abort,
    #[serde(rename = "compact")]
    #[allow(dead_code)]
    Compact,
    #[serde(rename = "follow_up")]
    FollowUp {
        text: &'a str,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        images: Vec<ImagePayload>,
    },
    #[serde(rename = "autocomplete_request")]
    AutocompleteRequest {
        id: u64,
        text: &'a str,
        cursor: usize,
        force: bool,
    },
    #[serde(rename = "cycle_model")]
    CycleModel { direction: &'a str },
    #[serde(rename = "cycle_thinking")]
    CycleThinking,
    #[serde(rename = "cycle_edit_mode")]
    CycleEditMode,
    #[serde(rename = "dialog_response")]
    DialogResponse {
        id: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        value: Option<&'a str>,
        #[serde(skip_serializing_if = "Option::is_none")]
        confirmed: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        cancelled: Option<bool>,
    },
    #[serde(rename = "permission_response")]
    PermissionResponse {
        id: &'a str,
        choice: &'a str,
        #[serde(rename = "trustKey", skip_serializing_if = "Option::is_none")]
        trust_key: Option<&'a str>,
    },
    #[allow(dead_code)]
    #[serde(rename = "steer")]
    Steer {
        text: &'a str,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        images: Vec<ImagePayload>,
    },
    #[serde(rename = "bash")]
    Bash { command: &'a str },
    #[serde(rename = "switch_agent")]
    SwitchAgent { id: &'a str },
    #[serde(rename = "switch_model")]
    SwitchModel { id: &'a str },
    #[serde(rename = "switch_session")]
    SwitchSession { path: &'a str },
    #[serde(rename = "delete_session")]
    DeleteSession { path: &'a str },
    #[serde(rename = "rename_session")]
    RenameSession { path: &'a str, name: &'a str },
    #[serde(rename = "list_sessions")]
    ListSessions { scope: &'a str },
    #[serde(rename = "list_models")]
    ListModels,
    #[serde(rename = "update_setting")]
    UpdateSetting {
        key: &'a str,
        value: &'a str,
    },
    #[serde(rename = "exit")]
    Exit,
}

pub fn send(
    writer: &Arc<Mutex<UnixStream>>,
    message: &ClientMessage<'_>,
) -> Result<(), Box<dyn Error>> {
    let mut stream = writer.lock().expect("writer lock poisoned");
    serde_json::to_writer(&mut *stream, message)?;
    stream.write_all(b"\n")?;
    stream.flush()?;
    Ok(())
}
