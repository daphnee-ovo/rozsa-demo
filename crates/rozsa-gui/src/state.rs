// File: state.rs
//
// GUI 多会话状态模型。
// 每个 session tab 有三种状态：Idle → Loaded → Active
// 只有 Active 状态的 session 有独立的 AgentSession 后端。

use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;

use serde::Serialize;
use tokio::sync::Mutex;

use rozsa_app::agent_session::AgentSession;
use rozsa_app::model_registry::ModelRegistry;
use rozsa_app::permissions::PendingApprovals;
use rozsa_core::events::AgentEvent;
use rozsa_core::messages::AgentMessage;

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
    pub pre_tool_use: Option<
        Arc<
            dyn Fn(
                    rozsa_core::config::PreToolUseContext,
                ) -> std::pin::Pin<
                    Box<
                        dyn std::future::Future<
                                Output = Option<rozsa_core::config::PreToolUseResult>,
                            > + Send,
                    >,
                > + Send
                + Sync,
        >,
    >,
}

/// 消息累积器
#[derive(Default)]
pub struct LiveState {
    pub messages: Vec<AgentMessage>,
    pub is_streaming: bool,
    pub turn_base: usize,
}

impl LiveState {
    pub fn apply(&mut self, event: &AgentEvent) -> bool {
        match event {
            AgentEvent::AgentStart => {
                self.turn_base = self.messages.len();
                self.is_streaming = true;
                true
            }
            AgentEvent::MessageStart { message } => {
                self.messages.push(message.clone());
                true
            }
            AgentEvent::MessageUpdate { message, .. } => {
                if let Some(last) = self.messages.last_mut() {
                    *last = message.clone();
                }
                true
            }
            AgentEvent::MessageEnd { message } => {
                if let Some(last) = self.messages.last_mut() {
                    *last = message.clone();
                }
                true
            }
            AgentEvent::AgentEnd { messages } => {
                self.messages.truncate(self.turn_base);
                self.messages.extend(messages.iter().cloned());
                self.is_streaming = false;
                true
            }
            AgentEvent::ToolExecutionStart { .. }
            | AgentEvent::ToolExecutionUpdate { .. }
            | AgentEvent::ToolExecutionEnd { .. } => true,
            _ => false,
        }
    }
}

/// 前端 UiSnapshot
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UiSnapshot {
    pub messages: Vec<serde_json::Value>,
    pub is_streaming: bool,
    pub model: Option<ModelInfo>,
    pub thinking_level: String,
    pub session_name: Option<String>,
    pub cwd: String,
    pub git: Option<GitStatus>,
    pub context_usage: ContextUsage,
    pub runtime_state: RuntimeState,
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
        let messages: Vec<serde_json::Value> = tab
            .messages()
            .iter()
            .filter_map(|m| serde_json::to_value(m).ok())
            .collect();

        let mut input_tokens: u64 = 0;
        let mut output_tokens: u64 = 0;
        for msg in tab.messages() {
            if let Some(rozsa_model::types::Message::Assistant(a)) = msg.as_standard() {
                input_tokens += a.usage.input;
                output_tokens += a.usage.output;
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

        let context_percent = if context_window > 0 {
            (input_tokens as f64 / context_window as f64) * 100.0
        } else {
            0.0
        };

        Self {
            messages,
            is_streaming: tab.is_streaming(),
            model: Some(model_info),
            thinking_level: thinking_str,
            session_name: None,
            cwd: shared.cwd.to_string_lossy().to_string(),
            git: git_status(&shared.cwd),
            context_usage: ContextUsage {
                percent: context_percent,
                tokens: input_tokens,
                context_window,
            },
            runtime_state: RuntimeState {
                prompt_tokens: input_tokens,
                completion_tokens: output_tokens,
                session_total_tokens: input_tokens + output_tokens,
            },
        }
    }
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
    let output = Command::new("git")
        .args([
            "-C",
            &cwd.to_string_lossy(),
            "diff",
            "--numstat",
            "HEAD",
            "--",
        ])
        .output();
    let Ok(output) = output else {
        return (0, 0);
    };
    if !output.status.success() {
        return (0, 0);
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let mut added = 0_u64;
    let mut deleted = 0_u64;
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        if let Some(raw) = parts.next().and_then(|value| value.parse::<u64>().ok()) {
            added += raw;
        }
        if let Some(raw) = parts.next().and_then(|value| value.parse::<u64>().ok()) {
            deleted += raw;
        }
    }
    (added, deleted)
}

/// 权限请求事件
#[derive(Clone, Serialize)]
pub struct PermissionEvent {
    pub id: String,
    pub tool: String,
    pub summary: String,
    pub risk: String,
    pub trust_key: String,
}

/// 工具执行事件
#[derive(Clone, Serialize)]
#[serde(tag = "type")]
pub enum ToolEvent {
    Start {
        id: String,
        name: String,
        args: serde_json::Value,
    },
    End {
        id: String,
        name: String,
        success: bool,
        output: String,
    },
}
