// File: state.rs
//
// GUI 多会话状态模型。
// 每个 session tab 有三种状态：Idle → Loaded → Active
// 只有 Active 状态的 session 有独立的 AgentSession 后端。

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use dashmap::DashMap;
use serde::Serialize;
use tokio::sync::Mutex;

use rozsa_app::agent_session::{AgentSession, AgentSessionConfig};
use rozsa_app::model_registry::ModelRegistry;
use rozsa_app::permissions::{PendingApprovals, PermissionResponse, TrustGroup, TrustLevel};
use rozsa_app::session::manager::SessionManager;
use rozsa_core::events::AgentEvent;
use rozsa_core::messages::AgentMessage;

pub use crate::turn_diff::{TurnActivity, TurnSummary, VerificationResult};

pub type PreToolUseHook = Arc<
    dyn Fn(
            rozsa_core::config::PreToolUseContext,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<Output = Option<rozsa_core::config::PreToolUseResult>>
                    + Send,
            >,
        > + Send
        + Sync,
>;

pub type PreToolUseHookFactory = Arc<dyn Fn(String) -> PreToolUseHook + Send + Sync>;

pub struct CreatedGuiSession {
    pub id: String,
    pub path: String,
    pub agent: AgentSession,
}

/// A permission request emitted by a session-owned pre-tool-use hook.
pub struct PermissionRequest {
    pub session_id: String,
    pub turn_id: String,
    pub request_id: String,
    pub tool_name: String,
    pub description: String,
    pub args: serde_json::Value,
    pub info: rozsa_app::permissions::ApprovalInfo,
}

#[derive(Clone)]
pub struct PendingPermissionContext {
    pub tool_name: String,
    pub args: serde_json::Value,
    pub info: rozsa_app::permissions::ApprovalInfo,
}

pub fn permission_pending_key(session_id: &str, request_id: &str) -> String {
    format!("{session_id}:{request_id}")
}

pub fn session_id_from_path(path: &str) -> String {
    Path::new(path)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or_default()
        .to_string()
}

pub fn find_tab_index_by_session(tabs: &[SessionTab], session_id: &str) -> Option<usize> {
    tabs.iter().position(|tab| tab.session_id() == session_id)
}

/// Resolve outstanding approvals so a closed, deleted, aborted, or failed session
/// cannot leave its agent loop waiting on a sender that will never receive UI input.
pub fn deny_pending_approvals(approvals: &PendingApprovals, session_id: Option<&str>) -> usize {
    let prefix = session_id.map(|id| format!("{id}:"));
    let keys = approvals
        .iter()
        .filter_map(|entry| {
            prefix
                .as_deref()
                .map_or(true, |prefix| entry.key().starts_with(prefix))
                .then(|| entry.key().clone())
        })
        .collect::<Vec<_>>();
    let mut resolved = 0;
    for key in keys {
        if let Some((_, sender)) = approvals.remove(&key) {
            let _ = sender.send(PermissionResponse::Deny);
            resolved += 1;
        }
    }
    resolved
}

/// 单个 session tab 的状态
pub enum SessionTab {
    /// 只有元数据（从 session 目录扫描得到），未点击过
    Idle {
        path: String,
        name: String,
        modified: String,
        message_count: u32,
    },
    /// 点击过，从 .jsonl 加载了历史消息用于显示，但还没有 agent backend
    Loaded {
        path: String,
        messages: Vec<AgentMessage>,
    },
    /// 发过消息，有活跃的独立 agent backend
    Active {
        path: String,
        agent: Arc<AgentSession>,
        live: LiveState,
    },
}

impl SessionTab {
    pub fn path(&self) -> &str {
        match self {
            Self::Idle { path, .. } => path,
            Self::Loaded { path, .. } => path,
            Self::Active { path, .. } => path,
        }
    }

    pub fn session_id(&self) -> String {
        session_id_from_path(self.path())
    }

    pub fn messages(&self) -> &[AgentMessage] {
        match self {
            Self::Idle { .. } => &[],
            Self::Loaded { messages, .. } => messages,
            Self::Active { live, .. } => &live.messages,
        }
    }

    pub fn is_streaming(&self) -> bool {
        match self {
            Self::Active { live, .. } => live.is_streaming,
            _ => false,
        }
    }
}

/// Tauri managed state
#[derive(Clone)]
pub struct GuiState {
    /// 所有 session tabs（key = session file path）
    pub tabs: Arc<Mutex<Vec<SessionTab>>>,
    /// 当前活跃 tab 的 index
    pub active_tab: Arc<Mutex<usize>>,
    /// 创建新 agent backend 所需的共享资源
    pub shared: Arc<SharedResources>,
    pub model_registry: Option<Arc<ModelRegistry>>,
    pub session_dir: Option<PathBuf>,
    pub pending_approvals: Option<PendingApprovals>,
    pub pending_permission_contexts: Arc<DashMap<String, PendingPermissionContext>>,
    pub permission_controller: Arc<rozsa_app::permissions::PermissionController>,
    pub global_settings_path: Option<PathBuf>,
    pub runtime_settings: Arc<Mutex<rozsa_app::settings::Settings>>,
}

/// 创建新 AgentSession 所需的共享资源（不持有具体 session 状态）
pub struct SharedResources {
    pub cwd: PathBuf,
    pub settings_manager: rozsa_app::settings::SettingsManager,
    pub resources: rozsa_app::resources::LoadedResources,
    pub system_prompt: String,
    pub model: Mutex<rozsa_model::types::Model>,
    pub thinking_level: Mutex<rozsa_model::types::ThinkingLevel>,
    pub pre_tool_use_factory: Option<PreToolUseHookFactory>,
    pub model_stream: Option<rozsa_app::agent_session::ModelStream>,
}

impl SharedResources {
    /// Create a lazy session and AgentSession through the GUI-owned factory.
    pub async fn create_new_agent(
        &self,
        session_dir: &Path,
        parent_session: Option<String>,
    ) -> Result<CreatedGuiSession, String> {
        let id = uuid::Uuid::new_v4().to_string();
        let path = session_dir.join(format!("{id}.jsonl"));
        let session_manager = SessionManager::create_lazy(
            &path,
            id.clone(),
            self.cwd.to_string_lossy().to_string(),
            parent_session,
        );
        let agent = self.create_agent(session_manager).await;
        Ok(CreatedGuiSession {
            id,
            path: path.to_string_lossy().to_string(),
            agent,
        })
    }

    /// Restore a persisted session through the same factory used for new sessions.
    pub async fn restore_agent(&self, path: &Path) -> Result<AgentSession, String> {
        let session_manager = SessionManager::open(path).map_err(|error| error.to_string())?;
        Ok(self.create_agent(session_manager).await)
    }

    async fn create_agent(&self, session_manager: SessionManager) -> AgentSession {
        let session_id = session_manager.session_id().to_string();
        let session_cwd = PathBuf::from(session_manager.cwd());
        let model = self.model.lock().await.clone();
        let thinking_level = *self.thinking_level.lock().await;
        let pre_tool_use = self.pre_tool_use_factory.as_ref().map(|factory| {
            let hook = factory(session_id);
            Box::new(move |context| hook(context)) as _
        });

        let session = AgentSession::new(AgentSessionConfig {
            model,
            thinking_level,
            system_prompt: self.system_prompt.clone(),
            cwd: session_cwd,
            session_manager,
            settings_manager: self.settings_manager.clone(),
            resources: self.resources.clone(),
            pre_tool_use,
            model_stream: self.model_stream.clone(),
        });
        session.register_default_tools(&self.cwd).await;
        session
    }
}

/// 消息累积器
#[derive(Default)]
pub struct LiveState {
    pub messages: Vec<AgentMessage>,
    pub is_streaming: bool,
    pub turn_base: usize,
    pub turn_id: u64,
    pub turn_activity: TurnActivity,
    pub interaction_active: bool,
    pub completed_summary: Option<TurnActivity>,
    /// GUI-owned FIFO. One item is started only after the preceding prompt
    /// has fully returned, so it cannot be coalesced by AgentSession follow-ups.
    pub queued_messages: Vec<String>,
    /// Messages supplied through `AgentSession::steer`, shown separately while
    /// they wait for the next tool result.
    pub steering_conversation: Vec<SteeringConversationEntry>,
    pub(crate) streaming_message_index: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SteeringConversationEntry {
    pub text: String,
    pub delivered: bool,
}

impl LiveState {
    pub fn apply(&mut self, event: &AgentEvent) -> bool {
        match event {
            AgentEvent::AgentStart => {
                self.turn_base = self.messages.len();
                self.turn_id = self.turn_id.saturating_add(1);
                self.is_streaming = true;
                if !self.interaction_active {
                    self.begin_interaction();
                }
                self.streaming_message_index = None;
                true
            }
            AgentEvent::MessageStart { message } => {
                self.messages.push(message.clone());
                if is_assistant_message(message) {
                    self.streaming_message_index = Some(self.messages.len() - 1);
                }
                true
            }
            AgentEvent::MessageUpdate { message, .. } => {
                self.update_streaming_message(message);
                true
            }
            AgentEvent::MessageEnd { message } => {
                self.update_streaming_message(message);
                self.remove_delivered_steering_message(message);
                self.streaming_message_index = None;
                true
            }
            AgentEvent::AgentEnd { messages } => {
                self.messages.truncate(self.turn_base);
                self.messages.extend(messages.iter().cloned());
                // Keep the input in its running state while the command layer
                // starts the next FIFO item after this prompt returns.
                self.is_streaming = !self.queued_messages.is_empty();
                self.streaming_message_index = None;
                true
            }
            AgentEvent::ToolExecutionEnd {
                tool_name, result, ..
            } => {
                self.record_tool_activity(tool_name, result);
                false
            }
            AgentEvent::ToolExecutionStart { .. } | AgentEvent::ToolExecutionUpdate { .. } => false,
            _ => false,
        }
    }

    fn update_streaming_message(&mut self, message: &AgentMessage) {
        if let Some(index) = self
            .streaming_message_index
            .filter(|index| *index < self.messages.len())
        {
            self.messages[index] = message.clone();
            return;
        }

        if !is_assistant_message(message) {
            return;
        }

        if let Some(index) = self.messages.iter().rposition(is_assistant_message) {
            self.messages[index] = message.clone();
            self.streaming_message_index = Some(index);
        }
    }

    fn record_tool_activity(
        &mut self,
        tool_name: &str,
        result: &rozsa_model::types::ToolResultMessage,
    ) {
        let mut accumulator = crate::turn_diff::TurnDiffAccumulator::new();
        accumulator.merge_activity(&self.turn_activity);
        accumulator.merge_result(tool_name, result);
        self.turn_activity = accumulator.activity();
    }

    pub fn enqueue_message(&mut self, message: String) {
        self.queued_messages.push(message);
    }

    pub fn begin_interaction(&mut self) {
        self.interaction_active = true;
        self.turn_activity = TurnActivity::default();
        self.completed_summary = None;
    }

    pub fn finish_interaction(&mut self, summary: TurnActivity) {
        self.interaction_active = false;
        self.is_streaming = false;
        self.completed_summary = Some(summary.clone());
        self.turn_activity = summary;
    }

    pub fn take_next_queued_message(&mut self) -> Option<String> {
        (!self.queued_messages.is_empty()).then(|| self.queued_messages.remove(0))
    }

    pub fn add_steering_message(&mut self, message: String) {
        self.steering_conversation.push(SteeringConversationEntry {
            text: message,
            delivered: false,
        });
    }

    fn remove_delivered_steering_message(&mut self, message: &AgentMessage) {
        let Some(rozsa_model::types::Message::User(user)) = message.as_standard() else {
            return;
        };
        if !user
            .display_text
            .as_deref()
            .is_some_and(|text| text.starts_with("[steer] "))
        {
            return;
        }
        let delivered = user.content.text();
        if let Some(index) = self
            .steering_conversation
            .iter()
            .position(|entry| entry.text == delivered)
        {
            self.steering_conversation.remove(index);
        }
    }
}

fn is_assistant_message(message: &AgentMessage) -> bool {
    matches!(
        message.as_standard(),
        Some(rozsa_model::types::Message::Assistant(_))
    )
}

/// 前端 UiSnapshot
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UiSnapshot {
    pub session_id: String,
    pub turn_id: u64,
    pub messages: Vec<serde_json::Value>,
    pub is_streaming: bool,
    pub model: Option<ModelInfo>,
    pub thinking_level: String,
    pub session_name: Option<String>,
    pub cwd: String,
    pub git: Option<GitStatus>,
    pub context_usage: ContextUsage,
    pub runtime_state: RuntimeState,
    pub turn_activity: TurnActivity,
    pub turn_summaries: Vec<TurnSummary>,
    pub queued_messages: Vec<String>,
    pub steering_conversation: Vec<SteeringConversationEntry>,
    pub stream_update: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub id: String,
    pub provider: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextUsage {
    pub percent: f64,
    pub tokens: u64,
    pub context_window: usize,
    pub input_tokens: u64,
    pub uncached_input_tokens: u64,
    pub cached_input_tokens: u64,
    pub output_tokens: u64,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitStatus {
    pub label: String,
    pub project_name: String,
    pub branch: Option<String>,
    pub dirty: bool,
    pub added: u64,
    pub deleted: u64,
    pub files: u64,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeState {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub session_total_tokens: u64,
}

impl UiSnapshot {
    pub fn from_tab(tab: &SessionTab, shared: &SharedResources) -> Self {
        Self::build(tab, shared, false)
    }

    pub fn from_stream_update(tab: &SessionTab, shared: &SharedResources) -> Self {
        Self::build(tab, shared, true)
    }

    fn build(tab: &SessionTab, shared: &SharedResources, stream_update: bool) -> Self {
        let messages: Vec<serde_json::Value> = tab
            .messages()
            .iter()
            .filter_map(|m| serde_json::to_value(m).ok())
            .collect();

        let mut session_input_tokens: u64 = 0;
        let mut session_output_tokens: u64 = 0;
        for msg in tab.messages() {
            if let Some(rozsa_model::types::Message::Assistant(a)) = msg.as_standard() {
                let context_tokens = usage_context_tokens(&a.usage);
                session_input_tokens += context_tokens;
                session_output_tokens += a.usage.output;
            }
        }

        let model_guard = shared.model.try_lock().unwrap_or_else(|_| unreachable!());
        let model_info = ModelInfo {
            id: model_guard.id.clone(),
            provider: format!("{:?}", model_guard.provider),
        };
        let context_window = model_guard.context_window;
        drop(model_guard);

        let thinking = shared
            .thinking_level
            .try_lock()
            .unwrap_or_else(|_| unreachable!());
        let thinking_str = format!("{:?}", *thinking).to_lowercase();
        drop(thinking);

        let context_usage = context_usage_from_messages(tab.messages(), context_window);

        Self {
            session_id: tab.session_id(),
            turn_id: match tab {
                SessionTab::Active { live, .. } => live.turn_id,
                _ => 0,
            },
            messages,
            is_streaming: tab.is_streaming(),
            model: Some(model_info),
            thinking_level: thinking_str,
            session_name: None,
            cwd: shared.cwd.to_string_lossy().to_string(),
            git: (!stream_update).then(|| git_status(&shared.cwd)).flatten(),
            context_usage: context_usage.clone(),
            runtime_state: RuntimeState {
                prompt_tokens: context_usage.input_tokens,
                completion_tokens: context_usage.output_tokens,
                session_total_tokens: session_input_tokens + session_output_tokens,
            },
            turn_activity: match tab {
                SessionTab::Active { live, .. } if live.interaction_active => {
                    live.turn_activity.clone()
                }
                _ => TurnActivity::default(),
            },
            turn_summaries: latest_turn_summary(tab),
            queued_messages: match tab {
                SessionTab::Active { live, .. } => live.queued_messages.clone(),
                _ => Vec::new(),
            },
            steering_conversation: match tab {
                SessionTab::Active { live, .. } => live.steering_conversation.clone(),
                _ => Vec::new(),
            },
            stream_update,
        }
    }
}

pub fn context_usage_from_messages(
    messages: &[AgentMessage],
    context_window: usize,
) -> ContextUsage {
    let usage = messages
        .iter()
        .rev()
        .find_map(|message| match message.as_standard()? {
            rozsa_model::types::Message::Assistant(assistant) => Some(&assistant.usage),
            _ => None,
        });
    let (input_tokens, uncached_input_tokens, cached_input_tokens, output_tokens) = usage
        .map(|usage| {
            (
                usage_context_tokens(usage),
                usage.input,
                usage.cache_read + usage.cache_write,
                usage.output,
            )
        })
        .unwrap_or((0, 0, 0, 0));
    let percent = if context_window > 0 {
        (input_tokens as f64 / context_window as f64) * 100.0
    } else {
        0.0
    };
    ContextUsage {
        percent,
        tokens: input_tokens,
        context_window,
        input_tokens,
        uncached_input_tokens,
        cached_input_tokens,
        output_tokens,
    }
}

fn usage_context_tokens(usage: &rozsa_model::types::Usage) -> u64 {
    let reported_prompt_tokens = usage.total_tokens.saturating_sub(usage.output);
    if reported_prompt_tokens > 0 {
        reported_prompt_tokens
    } else {
        usage
            .input
            .saturating_add(usage.cache_read)
            .saturating_add(usage.cache_write)
    }
}

fn latest_turn_summary(tab: &SessionTab) -> Vec<TurnSummary> {
    let activity = match tab {
        SessionTab::Active { live, .. } => live.completed_summary.clone(),
        _ => SessionManager::open(tab.path())
            .ok()
            .and_then(|manager| crate::turn_diff::latest_persisted_summary(&manager)),
    };
    let Some(activity) = activity else {
        return Vec::new();
    };
    let Some(assistant_message_index) = tab.messages().iter().rposition(is_assistant_message)
    else {
        return Vec::new();
    };
    if activity.changed_files.is_empty() {
        return Vec::new();
    }
    vec![TurnSummary {
        assistant_message_index,
        activity,
    }]
}

fn git_status(cwd: &PathBuf) -> Option<GitStatus> {
    let project_name = cwd
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("workspace")
        .to_string();
    let status_output = Command::new("git")
        .args([
            "-C",
            &cwd.to_string_lossy(),
            "status",
            "--porcelain=v1",
            "--branch",
        ])
        .output()
        .ok()?;
    if !status_output.status.success() {
        return Some(GitStatus {
            label: project_name.clone(),
            project_name,
            branch: None,
            dirty: false,
            added: 0,
            deleted: 0,
            files: 0,
        });
    }

    let status = String::from_utf8_lossy(&status_output.stdout);
    let mut branch = None;
    let mut files = 0_u64;
    for line in status.lines() {
        if let Some(raw_branch) = line.strip_prefix("## ") {
            let name = raw_branch
                .split_whitespace()
                .next()
                .unwrap_or(raw_branch)
                .split("...")
                .next()
                .unwrap_or(raw_branch);
            branch = (!name.is_empty()).then(|| name.to_string());
            continue;
        }
        if !line.trim().is_empty() {
            files += 1;
        }
    }

    let (added, deleted) = git_diff_stat(cwd);
    let dirty = files > 0;
    let label = match branch.as_deref() {
        Some(branch) => format!("{project_name}({branch}{})", if dirty { "*" } else { "" }),
        None => project_name.clone(),
    };

    Some(GitStatus {
        label,
        project_name,
        branch,
        dirty,
        added,
        deleted,
        files,
    })
}

fn git_diff_stat(cwd: &PathBuf) -> (u64, u64) {
    crate::git_diff::workspace_diff_stat(cwd)
}

/// 权限请求事件
#[derive(Clone, Serialize)]
pub struct PermissionEvent {
    pub session_id: String,
    pub turn_id: String,
    pub request_id: String,
    pub tool: String,
    pub description: String,
    pub summary: String,
    pub risk: String,
    pub trust_key: String,
    pub trust_levels: Vec<TrustLevel>,
    pub trust_groups: Vec<TrustGroup>,
}

/// 工具执行事件
#[derive(Clone, Serialize)]
#[serde(tag = "type")]
pub enum ToolEvent {
    Start {
        session_id: String,
        turn_id: u64,
        id: String,
        name: String,
        args: serde_json::Value,
    },
    End {
        session_id: String,
        turn_id: u64,
        id: String,
        name: String,
        success: bool,
        output: String,
        details: serde_json::Value,
    },
}
