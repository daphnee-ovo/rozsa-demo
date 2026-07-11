//! Reconstructable per-user-turn file summaries from persisted ToolResult details.

use std::collections::BTreeMap;

use rozsa_app::tools::file_delta::{FileDelta, build_file_delta};
use rozsa_core::messages::AgentMessage;
use rozsa_model::types::{Message, ToolResultMessage};
use serde::{Deserialize, Serialize};

pub const INTERACTION_STARTED: &str = "gui_interaction_started";
pub const INTERACTION_SUMMARY: &str = "gui_interaction_summary";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnActivity {
    pub changed_files: Vec<String>,
    pub file_changes: Vec<FileDelta>,
    pub verification: Option<VerificationResult>,
    pub capture_complete: bool,
    pub capture_limitation: Option<String>,
}

impl Default for TurnActivity {
    fn default() -> Self {
        Self {
            changed_files: Vec::new(),
            file_changes: Vec::new(),
            verification: None,
            capture_complete: true,
            capture_limitation: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerificationResult {
    pub command: String,
    pub success: bool,
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    pub truncated: bool,
    pub duration_ms: u64,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnSummary {
    pub assistant_message_index: usize,
    pub activity: TurnActivity,
}

#[derive(Default)]
pub struct TurnDiffAccumulator {
    files: BTreeMap<String, (Option<String>, Option<String>)>,
    opaque_files: std::collections::BTreeSet<String>,
    verification: Option<VerificationResult>,
    capture_complete: bool,
    capture_limitation: Option<String>,
}

impl TurnDiffAccumulator {
    pub fn new() -> Self {
        Self {
            capture_complete: true,
            ..Default::default()
        }
    }

    pub fn merge_result(&mut self, tool_name: &str, result: &ToolResultMessage) {
        let deltas = serde_json::from_value::<Vec<FileDelta>>(
            result
                .details
                .get("file_deltas")
                .cloned()
                .unwrap_or_default(),
        )
        .unwrap_or_default();
        if deltas.is_empty() {
            if let Some(paths) = result
                .details
                .get("changed_files")
                .and_then(|value| value.as_array())
            {
                self.opaque_files.extend(
                    paths
                        .iter()
                        .filter_map(|value| value.as_str())
                        .map(str::to_string),
                );
            }
        } else {
            for delta in deltas {
                self.opaque_files.remove(&delta.path);
                self.files
                    .entry(delta.path)
                    .and_modify(|entry| entry.1 = delta.after.clone())
                    .or_insert((delta.before, delta.after));
            }
        }
        if result
            .details
            .get("capture_complete")
            .and_then(|value| value.as_bool())
            == Some(false)
        {
            self.capture_complete = false;
            self.capture_limitation = result
                .details
                .get("capture_limitation")
                .and_then(|value| value.as_str())
                .map(str::to_string);
        }
        if tool_name.eq_ignore_ascii_case("bash") {
            let details = &result.details;
            if let Some(command) = details.get("command").and_then(|value| value.as_str()) {
                self.verification = Some(VerificationResult {
                    command: command.to_string(),
                    success: details
                        .get("success")
                        .and_then(|value| value.as_bool())
                        .unwrap_or(false),
                    exit_code: details
                        .get("exit_code")
                        .and_then(|value| value.as_i64())
                        .map(|value| value as i32),
                    timed_out: details
                        .get("timed_out")
                        .and_then(|value| value.as_bool())
                        .unwrap_or(false),
                    truncated: details
                        .get("truncated")
                        .and_then(|value| value.as_bool())
                        .unwrap_or(false),
                    duration_ms: details
                        .get("duration_ms")
                        .and_then(|value| value.as_u64())
                        .unwrap_or(0),
                });
            }
        }
    }

    pub fn merge_activity(&mut self, activity: &TurnActivity) {
        for delta in &activity.file_changes {
            self.files
                .entry(delta.path.clone())
                .and_modify(|entry| entry.1 = delta.after.clone())
                .or_insert((delta.before.clone(), delta.after.clone()));
        }
        self.opaque_files
            .extend(activity.changed_files.iter().cloned());
        for delta in &activity.file_changes {
            self.opaque_files.remove(&delta.path);
        }
        if activity.verification.is_some() {
            self.verification = activity.verification.clone();
        }
        if !activity.capture_complete {
            self.capture_complete = false;
            self.capture_limitation = activity.capture_limitation.clone();
        }
    }

    pub fn activity(&self) -> TurnActivity {
        let file_changes = self
            .files
            .iter()
            .filter_map(|(path, (before, after))| {
                build_file_delta(path.clone(), before.clone(), after.clone())
            })
            .collect::<Vec<_>>();
        let mut changed_files = file_changes
            .iter()
            .map(|delta| delta.path.clone())
            .collect::<Vec<_>>();
        changed_files.extend(self.opaque_files.iter().cloned());
        changed_files.sort();
        changed_files.dedup();
        TurnActivity {
            changed_files,
            file_changes,
            verification: self.verification.clone(),
            capture_complete: self.capture_complete,
            capture_limitation: self.capture_limitation.clone(),
        }
    }

    fn is_empty(&self) -> bool {
        self.files.is_empty() && self.verification.is_none()
    }
}

pub fn summarize_messages(messages: &[AgentMessage]) -> Vec<TurnSummary> {
    let mut summaries = Vec::new();
    let mut accumulator = TurnDiffAccumulator::new();
    let mut last_assistant = None;
    for (index, message) in messages.iter().enumerate() {
        match message.as_standard() {
            Some(Message::User(_)) => {
                push_summary(&mut summaries, &accumulator, last_assistant);
                accumulator = TurnDiffAccumulator::new();
                last_assistant = None;
            }
            Some(Message::Assistant(_)) => last_assistant = Some(index),
            Some(Message::ToolResult(result)) => {
                accumulator.merge_result(&result.tool_name, result);
            }
            None => {}
        }
    }
    push_summary(&mut summaries, &accumulator, last_assistant);
    summaries
}

pub fn latest_persisted_summary(
    manager: &rozsa_app::session::manager::SessionManager,
) -> Option<TurnActivity> {
    let summary = manager.latest_custom(INTERACTION_SUMMARY)?;
    if manager
        .latest_custom(INTERACTION_STARTED)
        .is_some_and(|started| started.base.timestamp > summary.base.timestamp)
    {
        return None;
    }
    serde_json::from_value(summary.data?).ok()
}

pub fn persisted_interaction_activity(
    manager: &rozsa_app::session::manager::SessionManager,
) -> TurnActivity {
    use rozsa_app::session::manager::SessionEntry;

    let entries = manager.entries();
    let Some(start) = entries.iter().rposition(|entry| {
        matches!(entry, SessionEntry::Custom(custom) if custom.custom_type == INTERACTION_STARTED)
    }) else {
        return TurnActivity::default();
    };
    let mut accumulator = TurnDiffAccumulator::new();
    for entry in &entries[start + 1..] {
        if let SessionEntry::Message(message) = entry
            && let Message::ToolResult(result) = &message.message
        {
            accumulator.merge_result(&result.tool_name, result);
        }
    }
    accumulator.activity()
}

fn push_summary(
    summaries: &mut Vec<TurnSummary>,
    accumulator: &TurnDiffAccumulator,
    assistant_message_index: Option<usize>,
) {
    if !accumulator.is_empty()
        && let Some(assistant_message_index) = assistant_message_index
    {
        summaries.push(TurnSummary {
            assistant_message_index,
            activity: accumulator.activity(),
        });
    }
}
