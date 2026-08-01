// FrameworkTree
// state.rs
// ├── struct CreatedGuiSession
// ├── struct PermissionRequest
// ├── struct PendingPermissionContext
// ├── struct PendingUserQuestion
// ├── permission_pending_key()
// ├── user_question_pending_key()
// ├── session_id_from_path()
// ├── find_tab_index_by_session()
// ├── deny_pending_approvals()
// ├── cancel_pending_user_questions()
// ├── respond_pending_user_question()
// ├── enum SessionTab
// ├── impl SessionTab
// ├── path()
// ├── session_id()
// ├── messages()
// ├── is_streaming()
// ├── struct GuiState
// ├── struct SidebarSessionSnapshot
// ├── struct SidebarActionsSnapshot
// ├── struct SidebarSnapshot
// ├── impl GuiState
// ├── sidebar_snapshot()
// ├── struct SharedResources
// ├── impl SharedResources
// ├── registered_tool_metadata()
// ├── create_new_agent()
// ├── create_continued_agent()
// ├── restore_agent()
// ├── create_agent()
// ├── struct LiveState
// ├── struct SteeringConversationEntry
// ├── impl LiveState
// ├── apply()
// ├── update_streaming_message()
// ├── record_tool_activity()
// ├── enqueue_message()
// ├── clear_queued_messages()
// ├── begin_interaction()
// ├── finish_interaction()
// ├── take_next_queued_message()
// ├── add_steering_message()
// ├── remove_delivered_steering_message()
// ├── is_assistant_message()
// ├── struct UiSnapshot
// ├── struct ModelInfo
// ├── struct ContextUsage
// ├── struct GitStatus
// ├── struct RuntimeState
// ├── impl UiSnapshot
// ├── from_tab()
// ├── from_stream_update()
// ├── build()
// ├── session_display_name()
// ├── session_preview()
// ├── context_usage_from_messages()
// ├── usage_context_tokens()
// ├── latest_turn_summary()
// ├── git_status()
// ├── git_diff_stat()
// ├── struct PermissionEvent
// ├── struct UserQuestionEvent
// └── enum ToolEvent

// File: state.rs
//
// GUI 多会话状态模型。
// 每个 session tab 有三种状态：Idle → Loaded → Active
// 只有 Active 状态的 session 有独立的 AgentSession 后端。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, RwLock};

use dashmap::DashMap;
use serde::Serialize;
use tokio::sync::Mutex;

use rozsa_app::agent_session::{AgentSession, AgentSessionConfig};
use rozsa_app::model_registry::ModelRegistry;
use rozsa_app::permissions::{PendingApprovals, PermissionResponse, TrustGroup, TrustLevel};
use rozsa_app::session::manager::SessionManager;
use rozsa_app::tools::{
    AskUserQuestion, AskUserQuestionAnswer, AskUserQuestionRequestSender, AskUserQuestionResponse,
    validate_ask_user_question_answers,
};
use rozsa_core::events::AgentEvent;
use rozsa_core::messages::AgentMessage;

use crate::scene_router::SceneRouter;

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

pub struct PendingUserQuestion {
    pub questions: Vec<AskUserQuestion>,
    pub response_tx: tokio::sync::oneshot::Sender<AskUserQuestionResponse>,
}

pub type PendingUserQuestions = Arc<DashMap<String, PendingUserQuestion>>;

pub fn permission_pending_key(session_id: &str, request_id: &str) -> String {
    format!("{session_id}:{request_id}")
}

pub fn user_question_pending_key(session_id: &str, request_id: &str) -> String {
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
                .is_none_or(|prefix| entry.key().starts_with(prefix))
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

/// Resolve outstanding askUserQuestion requests so an aborted, closed, or
/// deleted session cannot leave its agent loop waiting on a frontend response.
pub fn cancel_pending_user_questions(
    questions: &PendingUserQuestions,
    session_id: Option<&str>,
) -> usize {
    let prefix = session_id.map(|id| format!("{id}:"));
    let keys = questions
        .iter()
        .filter_map(|entry| {
            prefix
                .as_deref()
                .is_none_or(|prefix| entry.key().starts_with(prefix))
                .then(|| entry.key().clone())
        })
        .collect::<Vec<_>>();
    let mut resolved = 0;
    for key in keys {
        if let Some((_, pending)) = questions.remove(&key) {
            let _ = pending.response_tx.send(AskUserQuestionResponse::Cancelled);
            resolved += 1;
        }
    }
    resolved
}

pub fn respond_pending_user_question(
    questions: &PendingUserQuestions,
    session_id: &str,
    request_id: &str,
    answers: std::collections::BTreeMap<String, AskUserQuestionAnswer>,
) -> Result<(), String> {
    let key = user_question_pending_key(session_id, request_id);
    let expected_questions = questions
        .get(&key)
        .map(|pending| pending.questions.clone())
        .ok_or_else(|| format!("No pending user question: {session_id}:{request_id}"))?;
    validate_ask_user_question_answers(&expected_questions, &answers)
        .map_err(|error| error.to_string())?;

    let (_, pending) = questions
        .remove(&key)
        .ok_or_else(|| format!("No pending user question: {session_id}:{request_id}"))?;
    pending
        .response_tx
        .send(AskUserQuestionResponse::Answered { answers })
        .map_err(|_| "User question response channel is closed".to_string())
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
        dev_flow_presentations: HashMap<String, rozsa_app::dev_flow::DevFlowToolPresentation>,
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
    /// Window-level Main/Settings scene shared by the two persistent WebViews.
    pub scene_router: Arc<Mutex<SceneRouter>>,
    /// 所有 session tabs（key = session file path）
    pub tabs: Arc<Mutex<Vec<SessionTab>>>,
    /// 当前活跃 tab 的 index
    pub active_tab: Arc<Mutex<usize>>,
    /// 创建新 agent backend 所需的共享资源
    pub shared: Arc<SharedResources>,
    /// Dev-flow runtime registry, activity wiring, and diagnostics.
    pub dev_flow: Arc<crate::dev_flow::DevFlowRuntime>,
    pub model_registry: Option<Arc<RwLock<ModelRegistry>>>,
    pub session_dir: Option<PathBuf>,
    pub session_dirs: Vec<PathBuf>,
    pub config_roots: rozsa_app::config_paths::ConfigRoots,
    pub pending_approvals: Option<PendingApprovals>,
    pub pending_permission_contexts: Arc<DashMap<String, PendingPermissionContext>>,
    pub pending_user_questions: PendingUserQuestions,
    pub permission_controller: Arc<rozsa_app::permissions::PermissionController>,
    pub global_settings_path: Option<PathBuf>,
    pub runtime_settings: Arc<Mutex<rozsa_app::settings::Settings>>,
    /// Serializes Dev-flow settings persistence and runtime reconfiguration.
    pub dev_flow_settings_update: Arc<Mutex<()>>,
    pub quota_summary: Arc<Mutex<Option<rozsa_model::rate_limit::RateLimitSnapshot>>>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SidebarSessionSnapshot {
    pub id: String,
    pub path: String,
    pub name: String,
    pub modified: String,
    pub message_count: u32,
    pub activity: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SidebarActionsSnapshot {
    pub can_new_session: bool,
    pub can_rename_session: bool,
    pub can_delete_session: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SidebarSnapshot {
    pub sessions: Vec<SidebarSessionSnapshot>,
    pub active_session_id: Option<String>,
    pub dev_flow: Option<crate::dev_flow::DevFlowSidebarSnapshot>,
    pub show_dev_flow_dashboard: bool,
    pub git: Option<GitStatus>,
    pub quota: Option<rozsa_model::rate_limit::RateLimitSnapshot>,
    pub show_quota: bool,
    pub show_hourly_quota: bool,
    pub show_weekly_quota: bool,
    pub rate_limit_display_mode: String,
    pub actions: SidebarActionsSnapshot,
}

impl GuiState {
    pub async fn sidebar_snapshot(&self) -> Result<SidebarSnapshot, String> {
        if self.session_dirs.is_empty() {
            return Err("No session directories configured".to_string());
        }
        let metas =
            SessionManager::list_dirs(&self.session_dirs).map_err(|error| error.to_string())?;
        let active_index = *self.active_tab.lock().await;
        let tabs = self.tabs.lock().await;
        let active_session_id = tabs.get(active_index).map(SessionTab::session_id);
        let sessions = metas
            .into_iter()
            .map(|meta| {
                let activity = tabs
                    .iter()
                    .find(|tab| tab.session_id() == meta.id)
                    .map(|tab| {
                        let approval_prefix = format!("{}:", meta.id);
                        if self
                            .pending_permission_contexts
                            .iter()
                            .any(|entry| entry.key().starts_with(&approval_prefix))
                        {
                            "approval"
                        } else if tab.is_streaming() {
                            "running"
                        } else {
                            "idle"
                        }
                    })
                    .unwrap_or("idle")
                    .to_owned();
                let name = meta
                    .name
                    .filter(|name| !name.trim().is_empty())
                    .unwrap_or_else(|| session_preview(&meta.first_message));
                SidebarSessionSnapshot {
                    id: meta.id,
                    path: meta.path.to_string_lossy().to_string(),
                    name,
                    modified: meta.modified,
                    message_count: meta.message_count,
                    activity,
                }
            })
            .collect();
        drop(tabs);
        let dev_flow_settings = self.runtime_settings.lock().await.dev_flow.clone();
        let show_dev_flow_dashboard =
            dev_flow_settings.enabled && dev_flow_settings.show_dashboard_button;
        let dev_flow = {
            if dev_flow_settings.enabled {
                match &active_session_id {
                    Some(session_id) => {
                        self.dev_flow
                            .sidebar_snapshot(session_id, dev_flow_settings.show_sidebar_status)
                            .await
                    }
                    None => None,
                }
            } else {
                None
            }
        };
        let has_active_session = active_session_id.is_some();
        let appearance = self.runtime_settings.lock().await.appearance.clone();
        let show_quota = appearance.show_rate_limits
            && self.shared.model.lock().await.provider.as_str() == "codex-oauth";
        let show_weekly_quota = show_quota && appearance.show_weekly_rate_limit;
        let show_hourly_quota = show_quota && appearance.show_hourly_rate_limit;
        Ok(SidebarSnapshot {
            sessions,
            active_session_id,
            dev_flow,
            show_dev_flow_dashboard,
            git: git_status(&self.shared.cwd),
            quota: if show_quota {
                self.quota_summary.lock().await.clone()
            } else {
                None
            },
            show_quota,
            show_hourly_quota,
            show_weekly_quota,
            rate_limit_display_mode: appearance.rate_limit_display_mode,
            actions: SidebarActionsSnapshot {
                can_new_session: true,
                can_rename_session: has_active_session,
                can_delete_session: has_active_session,
            },
        })
    }
}

/// 创建新 AgentSession 所需的共享资源（不持有具体 session 状态）
pub struct SharedResources {
    pub cwd: PathBuf,
    pub settings_manager: rozsa_app::settings::SettingsManager,
    pub resources: rozsa_app::resources::LoadedResources,
    pub system_prompt: String,
    pub model: Mutex<rozsa_model::types::Model>,
    pub thinking_effort: Mutex<rozsa_model::types::ThinkingEffort>,
    pub pre_tool_use_factory: Option<PreToolUseHookFactory>,
    pub question_request_tx: Option<AskUserQuestionRequestSender>,
    pub model_stream: Option<rozsa_app::agent_session::ModelStream>,
}

impl SharedResources {
    /// Build the same tool catalog used by real GUI sessions without persisting
    /// a session. Settings must remain inspectable before the first chat exists.
    pub async fn registered_tool_metadata(
        &self,
    ) -> Result<Vec<rozsa_core::tool::ToolMetadata>, String> {
        let session_manager = SessionManager::create_lazy(
            self.cwd.join(".rozsa-tool-catalog.jsonl"),
            "settings-tool-catalog".to_owned(),
            self.cwd.to_string_lossy().to_string(),
            None,
        );
        let agent = self.create_agent(session_manager).await?;
        Ok(agent.registered_tool_metadata().await)
    }

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
        let agent = self.create_agent(session_manager).await?;
        Ok(CreatedGuiSession {
            id,
            path: path.to_string_lossy().to_string(),
            agent,
        })
    }

    /// Create a child session that continues the persisted parent context.
    pub async fn create_continued_agent(
        &self,
        session_dir: &Path,
        parent_session: &Path,
    ) -> Result<CreatedGuiSession, String> {
        let id = uuid::Uuid::new_v4().to_string();
        let path = session_dir.join(format!("{id}.jsonl"));
        let mut session_manager = SessionManager::create_lazy(
            &path,
            id.clone(),
            self.cwd.to_string_lossy().to_string(),
            Some(parent_session.to_string_lossy().to_string()),
        );
        session_manager
            .copy_context_messages_from_path(parent_session)
            .map_err(|error| error.to_string())?;
        let agent = self.create_agent(session_manager).await?;
        Ok(CreatedGuiSession {
            id,
            path: path.to_string_lossy().to_string(),
            agent,
        })
    }

    /// Restore a persisted session through the same factory used for new sessions.
    pub async fn restore_agent(&self, path: &Path) -> Result<AgentSession, String> {
        let session_manager = SessionManager::open(path).map_err(|error| error.to_string())?;
        self.create_agent(session_manager).await
    }

    async fn create_agent(&self, session_manager: SessionManager) -> Result<AgentSession, String> {
        let session_id = session_manager.session_id().to_string();
        let session_cwd = PathBuf::from(session_manager.cwd());
        let model = self.model.lock().await.clone();
        let thinking_effort = *self.thinking_effort.lock().await;
        let pre_tool_use = self.pre_tool_use_factory.as_ref().map(|factory| {
            let hook = factory(session_id.clone());
            Box::new(move |context| hook(context)) as _
        });

        let mut settings_manager = self.settings_manager.clone();
        settings_manager
            .reload()
            .map_err(|error| error.to_string())?;
        let session = AgentSession::new(AgentSessionConfig {
            model,
            thinking_effort,
            system_prompt: self.system_prompt.clone(),
            cwd: session_cwd,
            session_manager,
            settings_manager,
            resources: self.resources.clone(),
            pre_tool_use,
            model_stream: self.model_stream.clone(),
        });
        session
            .register_default_tools_with_question_sender(
                &self.cwd,
                self.question_request_tx
                    .clone()
                    .map(|sender| (session_id, sender)),
            )
            .await;
        Ok(session)
    }
}

/// 消息累积器
#[derive(Default)]
pub struct LiveState {
    pub messages: Vec<AgentMessage>,
    pub dev_flow_presentations: HashMap<String, rozsa_app::dev_flow::DevFlowToolPresentation>,
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

    pub fn clear_queued_messages(&mut self) {
        self.queued_messages.clear();
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
    pub dev_flow_presentations: HashMap<String, rozsa_app::dev_flow::DevFlowToolPresentation>,
    pub is_streaming: bool,
    pub model: Option<ModelInfo>,
    pub thinking_effort: String,
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
            .thinking_effort
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
            dev_flow_presentations: match tab {
                SessionTab::Loaded {
                    dev_flow_presentations,
                    ..
                } => dev_flow_presentations.clone(),
                SessionTab::Active { live, .. } => live.dev_flow_presentations.clone(),
                SessionTab::Idle { .. } => HashMap::new(),
            },
            is_streaming: tab.is_streaming(),
            model: Some(model_info),
            thinking_effort: thinking_str,
            // Stream updates retain the existing header. Full snapshots resolve
            // the latest persisted name, then fall back to the first user turn.
            session_name: (!stream_update).then(|| session_display_name(tab)),
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

/// Resolve the selected session's user-facing title without conflating the
/// durable name with its deterministic first-message preview.
pub fn session_display_name(tab: &SessionTab) -> String {
    if let Ok(manager) = SessionManager::open(tab.path())
        && let Some(name) = manager
            .current_name()
            .filter(|name| !name.trim().is_empty())
    {
        return name;
    }
    if let SessionTab::Idle { name, .. } = tab
        && !name.trim().is_empty()
    {
        return name.clone();
    }
    let first_user_message = tab.messages().iter().find_map(|message| {
        let rozsa_model::types::Message::User(user) = message.as_standard()? else {
            return None;
        };
        let text = user
            .display_text
            .clone()
            .unwrap_or_else(|| user.content.text());
        (!text.trim().is_empty()).then_some(text)
    });
    first_user_message.map_or_else(
        || "Untitled".to_string(),
        |message| session_preview(&message),
    )
}

fn session_preview(message: &str) -> String {
    let message = message.trim();
    if message.is_empty() {
        return "Untitled".to_string();
    }
    let preview = message.chars().take(50).collect::<String>();
    if message.chars().count() > 50 {
        format!("{preview}...")
    } else {
        preview
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

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserQuestionEvent {
    pub session_id: String,
    pub request_id: String,
    pub questions: Vec<AskUserQuestion>,
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
