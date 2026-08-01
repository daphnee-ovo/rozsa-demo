// FrameworkTree
// command.rs
// ├── enum DevFlowPresentationAction
// ├── enum DevFlowPresentationItemKind
// ├── struct DevFlowPresentationItem
// ├── struct DevFlowToolPresentation
// ├── struct BashExecutionEvidence
// ├── enum DevFlowRecordedRevision
// ├── struct DevFlowPresentationRecord
// ├── impl DevFlowPresentationRecord
// ├── new()
// ├── matches_project()
// ├── impl DevFlowRecordedRevision
// ├── from()
// ├── enum Quote
// ├── enum ShellToken
// ├── recognize_dow_bash()
// ├── rebuild_dev_flow_presentations()
// ├── persisted_bash_evidence()
// ├── enrich_titles()
// ├── split_pipeline()
// ├── tokenize_final_stage()
// ├── remove_input_redirections()
// ├── is_supported_executable()
// ├── parse_created_ids()
// ├── parse_argument_ids()
// ├── normalize_id()
// └── short_id()

//! Side-effect-free recognition of the small `dow` shell grammar that Rózsa
//! can present as structured Dev-flow actions.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use rozsa_model::types::{ContentBlock, Message};
use serde::{Deserialize, Serialize};

use super::{DevFlowProjectKey, DevFlowRevisionKey, DevFlowSnapshot};

pub const DEV_FLOW_PRESENTATION_CUSTOM_TYPE: &str = "dev_flow_presentation";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum DevFlowPresentationAction {
    Created,
    Claimed,
    Completed,
    Closed,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DevFlowPresentationItemKind {
    Task,
    Issue,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DevFlowPresentationItem {
    pub kind: DevFlowPresentationItemKind,
    pub id: String,
    pub short_id: String,
    pub title: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DevFlowToolPresentation {
    pub action: DevFlowPresentationAction,
    pub items: Vec<DevFlowPresentationItem>,
    pub details_unavailable: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BashExecutionEvidence {
    pub success: bool,
    pub exit_code: Option<i32>,
    pub truncated: bool,
    pub stdout: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "kebab-case")]
pub enum DevFlowRecordedRevision {
    NamedBranch(String),
    UnbornBranch(String),
    DetachedCommit(String),
    NonGit,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DevFlowPresentationRecord {
    pub tool_call_id: String,
    pub execution_root: PathBuf,
    pub execution_revision: DevFlowRecordedRevision,
    pub presentation: DevFlowToolPresentation,
    pub timestamp: i64,
}

impl DevFlowPresentationRecord {
    pub fn new(
        tool_call_id: String,
        project: &DevFlowProjectKey,
        presentation: DevFlowToolPresentation,
        timestamp: i64,
    ) -> Self {
        Self {
            tool_call_id,
            execution_root: project.root.clone(),
            execution_revision: DevFlowRecordedRevision::from(&project.revision),
            presentation,
            timestamp,
        }
    }

    pub fn matches_project(&self, project: &DevFlowProjectKey) -> bool {
        self.execution_root == project.root
            && self.execution_revision == DevFlowRecordedRevision::from(&project.revision)
    }
}

impl From<&DevFlowRevisionKey> for DevFlowRecordedRevision {
    fn from(revision: &DevFlowRevisionKey) -> Self {
        match revision {
            DevFlowRevisionKey::NamedBranch(value) => Self::NamedBranch(value.clone()),
            DevFlowRevisionKey::UnbornBranch(value) => Self::UnbornBranch(value.clone()),
            DevFlowRevisionKey::DetachedCommit(value) => Self::DetachedCommit(value.clone()),
            DevFlowRevisionKey::NonGit => Self::NonGit,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Quote {
    None,
    Single,
    Double,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum ShellToken {
    Word(String),
    InputRedirect,
}

/// Recognize a confirmed successful Bash execution without executing or
/// reading anything. Unsupported syntax deliberately returns `None` so the
/// caller keeps the generic Bash presentation.
pub fn recognize_dow_bash(
    command: &str,
    validated_dow: Option<&Path>,
    evidence: &BashExecutionEvidence,
) -> Option<DevFlowToolPresentation> {
    if !evidence.success || evidence.exit_code != Some(0) || evidence.truncated {
        return None;
    }
    let stages = split_pipeline(command)?;
    let final_tokens = tokenize_final_stage(stages.last()?)?;
    let words = remove_input_redirections(final_tokens)?;
    let executable = words.first()?;
    if !is_supported_executable(executable, validated_dow) {
        return None;
    }
    let args = &words[1..];
    let (action, expected_kind, ids) = match args {
        [scope, operation] if operation == "create" && scope == "task" => (
            DevFlowPresentationAction::Created,
            Some(DevFlowPresentationItemKind::Task),
            parse_created_ids(&evidence.stdout, DevFlowPresentationItemKind::Task)?,
        ),
        [scope, operation] if operation == "create" && scope == "issue" => (
            DevFlowPresentationAction::Created,
            Some(DevFlowPresentationItemKind::Issue),
            parse_created_ids(&evidence.stdout, DevFlowPresentationItemKind::Issue)?,
        ),
        [operation, tail @ ..] if operation == "claim" => (
            DevFlowPresentationAction::Claimed,
            None,
            parse_argument_ids(tail, None, true)?,
        ),
        [scope, operation, tail @ ..] if scope == "task" && operation == "done" => (
            DevFlowPresentationAction::Completed,
            Some(DevFlowPresentationItemKind::Task),
            parse_argument_ids(tail, Some(DevFlowPresentationItemKind::Task), false)?,
        ),
        [scope, operation, tail @ ..] if scope == "issue" && operation == "close" => (
            DevFlowPresentationAction::Closed,
            Some(DevFlowPresentationItemKind::Issue),
            parse_argument_ids(tail, Some(DevFlowPresentationItemKind::Issue), false)?,
        ),
        _ => return None,
    };
    if stages.len() > 1 && action != DevFlowPresentationAction::Created {
        return None;
    }
    let items = ids
        .into_iter()
        .map(|(kind, id)| DevFlowPresentationItem {
            kind: expected_kind.unwrap_or(kind),
            short_id: short_id(&id),
            id,
            title: None,
        })
        .collect::<Vec<_>>();
    Some(DevFlowToolPresentation {
        action,
        details_unavailable: items.iter().any(|item| item.title.is_none()),
        items,
    })
}

/// Rebuild presentation state from persisted evidence only. Records without a
/// matching persisted Bash ToolCall and ToolResult are ignored. A live
/// snapshot can enrich missing titles only for the exact execution-time key.
pub fn rebuild_dev_flow_presentations(
    records: impl IntoIterator<Item = DevFlowPresentationRecord>,
    messages: &[Message],
    enrichment: Option<(&DevFlowProjectKey, &DevFlowSnapshot)>,
) -> HashMap<String, DevFlowToolPresentation> {
    let (tool_calls, tool_results) = persisted_bash_evidence(messages);
    let mut records = records.into_iter().collect::<Vec<_>>();
    records.sort_by_key(|record| record.timestamp);
    let mut rebuilt = HashMap::new();
    for record in records {
        if !tool_calls.contains(&record.tool_call_id)
            || !tool_results.contains(&record.tool_call_id)
        {
            continue;
        }
        let matching_enrichment = enrichment
            .filter(|(project, _)| record.matches_project(project))
            .map(|(_, snapshot)| snapshot);
        let mut presentation = record.presentation;
        if let Some(snapshot) = matching_enrichment {
            enrich_titles(&mut presentation, snapshot);
        }
        presentation.details_unavailable =
            presentation.items.iter().any(|item| item.title.is_none());
        rebuilt.insert(record.tool_call_id, presentation);
    }
    rebuilt
}

fn persisted_bash_evidence(messages: &[Message]) -> (HashSet<String>, HashSet<String>) {
    let mut calls = HashSet::new();
    let mut results = HashSet::new();
    for message in messages {
        match message {
            Message::Assistant(assistant) => {
                for block in &assistant.content {
                    if let ContentBlock::ToolCall(call) = block
                        && call.name == "bash"
                    {
                        calls.insert(call.id.clone());
                    }
                }
            }
            Message::ToolResult(result) if result.tool_name == "bash" => {
                results.insert(result.tool_call_id.clone());
            }
            _ => {}
        }
    }
    (calls, results)
}

fn enrich_titles(presentation: &mut DevFlowToolPresentation, snapshot: &DevFlowSnapshot) {
    for item in &mut presentation.items {
        if item.title.is_some() {
            continue;
        }
        item.title = match item.kind {
            DevFlowPresentationItemKind::Task => snapshot
                .tasks
                .iter()
                .find(|task| task.id == item.id)
                .map(|task| task.title.clone()),
            DevFlowPresentationItemKind::Issue => snapshot
                .issues
                .iter()
                .find(|issue| issue.id == item.id)
                .map(|issue| issue.title.clone()),
        };
    }
}

fn split_pipeline(command: &str) -> Option<Vec<String>> {
    let mut quote = Quote::None;
    let mut escaped = false;
    let mut stages = vec![String::new()];
    let mut chars = command.chars().peekable();
    while let Some(ch) = chars.next() {
        if escaped {
            if ch == '\n' || ch == '\r' {
                return None;
            }
            stages.last_mut()?.push(ch);
            escaped = false;
            continue;
        }
        match quote {
            Quote::Single => {
                stages.last_mut()?.push(ch);
                if ch == '\'' {
                    quote = Quote::None;
                }
            }
            Quote::Double => {
                stages.last_mut()?.push(ch);
                if ch == '"' {
                    quote = Quote::None;
                } else if ch == '\\' {
                    escaped = true;
                }
            }
            Quote::None => match ch {
                '\'' => {
                    quote = Quote::Single;
                    stages.last_mut()?.push(ch);
                }
                '"' => {
                    quote = Quote::Double;
                    stages.last_mut()?.push(ch);
                }
                '\\' => {
                    stages.last_mut()?.push(ch);
                    escaped = true;
                }
                '\n' | '\r' | ';' | '&' | '`' => return None,
                '$' if chars.peek() == Some(&'(') => return None,
                '|' => {
                    if chars.peek() == Some(&'|') || stages.last()?.trim().is_empty() {
                        return None;
                    }
                    stages.push(String::new());
                }
                _ => stages.last_mut()?.push(ch),
            },
        }
    }
    if escaped || quote != Quote::None || stages.last()?.trim().is_empty() {
        return None;
    }
    Some(stages)
}

fn tokenize_final_stage(stage: &str) -> Option<Vec<ShellToken>> {
    let mut quote = Quote::None;
    let mut escaped = false;
    let mut word = String::new();
    let mut word_started = false;
    let mut tokens = Vec::new();
    let flush_word = |tokens: &mut Vec<ShellToken>, word: &mut String, started: &mut bool| {
        if *started {
            tokens.push(ShellToken::Word(std::mem::take(word)));
            *started = false;
        }
    };
    for ch in stage.chars() {
        if escaped {
            word.push(ch);
            word_started = true;
            escaped = false;
            continue;
        }
        match quote {
            Quote::Single => {
                if ch == '\'' {
                    quote = Quote::None;
                } else {
                    word.push(ch);
                }
                word_started = true;
            }
            Quote::Double => {
                if ch == '"' {
                    quote = Quote::None;
                } else if ch == '\\' {
                    escaped = true;
                } else {
                    word.push(ch);
                }
                word_started = true;
            }
            Quote::None => match ch {
                '\'' => {
                    quote = Quote::Single;
                    word_started = true;
                }
                '"' => {
                    quote = Quote::Double;
                    word_started = true;
                }
                '\\' => escaped = true,
                '<' => {
                    flush_word(&mut tokens, &mut word, &mut word_started);
                    tokens.push(ShellToken::InputRedirect);
                }
                '>' => return None,
                ch if ch.is_whitespace() => {
                    flush_word(&mut tokens, &mut word, &mut word_started);
                }
                _ => {
                    word.push(ch);
                    word_started = true;
                }
            },
        }
    }
    if escaped || quote != Quote::None {
        return None;
    }
    flush_word(&mut tokens, &mut word, &mut word_started);
    Some(tokens)
}

fn remove_input_redirections(tokens: Vec<ShellToken>) -> Option<Vec<String>> {
    let mut words = Vec::new();
    let mut tokens = tokens.into_iter();
    while let Some(token) = tokens.next() {
        match token {
            ShellToken::Word(word) => words.push(word),
            ShellToken::InputRedirect => match tokens.next()? {
                ShellToken::Word(path) if !path.is_empty() => {}
                _ => return None,
            },
        }
    }
    Some(words)
}

fn is_supported_executable(word: &str, validated_dow: Option<&Path>) -> bool {
    if word == "dow" {
        return true;
    }
    let candidate = Path::new(word);
    candidate.is_absolute() && validated_dow == Some(candidate)
}

fn parse_created_ids(
    stdout: &str,
    expected: DevFlowPresentationItemKind,
) -> Option<Vec<(DevFlowPresentationItemKind, String)>> {
    let ids = stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| normalize_id(line.trim()))
        .collect::<Option<Vec<_>>>()?;
    if ids.is_empty() || ids.iter().any(|(kind, _)| *kind != expected) {
        return None;
    }
    Some(ids)
}

fn parse_argument_ids(
    args: &[String],
    expected: Option<DevFlowPresentationItemKind>,
    allow_timeout: bool,
) -> Option<Vec<(DevFlowPresentationItemKind, String)>> {
    let mut ids = Vec::new();
    let mut index = 0;
    while index < args.len() {
        if allow_timeout && args[index] == "--timeout" {
            let timeout = args.get(index + 1)?;
            if timeout
                .parse::<u64>()
                .ok()
                .filter(|value| *value > 0)
                .is_none()
            {
                return None;
            }
            index += 2;
            continue;
        }
        let parsed = normalize_id(&args[index])?;
        if expected.is_some_and(|kind| kind != parsed.0) {
            return None;
        }
        ids.push(parsed);
        index += 1;
    }
    (!ids.is_empty()).then_some(ids)
}

fn normalize_id(value: &str) -> Option<(DevFlowPresentationItemKind, String)> {
    let (kind, digits) = if let Some(digits) = value.strip_prefix("TASK-T") {
        (DevFlowPresentationItemKind::Task, digits)
    } else if let Some(digits) = value.strip_prefix('T') {
        (DevFlowPresentationItemKind::Task, digits)
    } else if let Some(digits) = value.strip_prefix("ISSUE-I") {
        (DevFlowPresentationItemKind::Issue, digits)
    } else if let Some(digits) = value.strip_prefix('I') {
        (DevFlowPresentationItemKind::Issue, digits)
    } else {
        return None;
    };
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let number = digits.parse::<u64>().ok()?;
    if number == 0 {
        return None;
    }
    let canonical = match kind {
        DevFlowPresentationItemKind::Task => format!("TASK-T{number:03}"),
        DevFlowPresentationItemKind::Issue => format!("ISSUE-I{number:03}"),
    };
    Some((kind, canonical))
}

fn short_id(id: &str) -> String {
    id.strip_prefix("TASK-")
        .or_else(|| id.strip_prefix("ISSUE-"))
        .unwrap_or(id)
        .to_owned()
}
