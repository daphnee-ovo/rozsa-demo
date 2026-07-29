// FrameworkTree
// manager.rs
// ├── struct SharedResources
// ├── struct SpawnConfig
// ├── struct SubagentSnapshot
// ├── struct SubagentManager
// ├── impl SubagentManager
// ├── new()
// ├── active_count()
// ├── spawn()
// ├── send()
// ├── wait()
// ├── abort()
// ├── list()
// ├── tool_names()
// ├── list_sync()
// ├── get_messages()
// ├── snapshot()
// ├── build_loop_config()
// ├── drain_subagent_stream()
// ├── resolve_api_key()
// └── current_timestamp_ms()

// File: subagent/manager.rs
//
// SubagentManager — spawns, sends, waits, aborts subagents.
//
// Each subagent owns its own message history, scope (tool whitelist + path
// constraints), and optional session file. The manager keeps a hashmap of
// active subagents and exposes a small surface for backends to drive them.
//
// Internal Framework:
// manager.rs
// ├── SharedResources       # cloned references to main session deps
// ├── SpawnConfig
// ├── SubagentSnapshot
// └── SubagentManager
//     ├── new()
//     ├── spawn()           # create a new subagent
//     ├── send()            # deliver a user prompt; runs agent_loop
//     ├── wait()            # await status != Running
//     ├── abort()           # cancel running loop
//     ├── list()
//     ├── get_messages()
//     └── snapshot()
//
// Related Code:
// - [runtime.rs](./runtime.rs)
// - [scope.rs](./scope.rs)
// - [agent_session.rs](../agent_session.rs)

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::Arc;

use rozsa_core::agent_loop::agent_loop;
use rozsa_core::config::{
    AgentContext, AgentLoopConfig, ModelStreamFn, PreToolUseContext, PreToolUseResult,
};
use rozsa_core::events::AgentEvent;
use rozsa_core::messages::AgentMessage;
use rozsa_core::tool::{Tool, ToolExecutionMode};
use rozsa_model::types::{
    CacheRetention, Message, Model, SimpleStreamOptions, StreamEvent, StreamOptions, ThinkingLevel,
    ToolSchema, Transport, UserContent, UserMessage,
};
use tokio::sync::{Mutex, watch};
use tokio_util::sync::CancellationToken;

use super::runtime::{SharedRuntime, SubagentInfo, SubagentRuntime, SubagentStatus};
use super::scope::{AllowedTools, SubagentScope};
use crate::session::manager::SessionManager;

const MAX_ACTIVE_SUBAGENTS: usize = 10;
const SUBAGENT_BLOCKED_TOOLS: &[&str] = &["subagent"];

/// Convert AgentMessages to LLM-compatible Messages, filtering out custom messages.
pub type ConvertToLlmFn = Arc<dyn Fn(&[AgentMessage]) -> Vec<Message> + Send + Sync>;

/// Stream factory shared with the main agent loop.
pub type ModelStreamArc = Arc<
    dyn Fn(
            &Model,
            &rozsa_model::types::Context,
            &SimpleStreamOptions,
        ) -> rozsa_model::event_stream::EventStream<StreamEvent>
        + Send
        + Sync,
>;

/// Pre-tool-use hook type shared with subagents (same signature as main agent's hook).
pub type PreToolUseHook = Arc<
    dyn Fn(
            PreToolUseContext,
        )
            -> std::pin::Pin<Box<dyn std::future::Future<Output = Option<PreToolUseResult>> + Send>>
        + Send
        + Sync,
>;

/// Shared resources cloned into each subagent's agent_loop call.
pub struct SharedResources {
    pub model_stream: ModelStreamArc,
    pub convert_to_llm: ConvertToLlmFn,
    pub main_tools: Arc<Mutex<Vec<Arc<dyn Tool>>>>,
    pub tool_settings: Arc<std::sync::RwLock<BTreeMap<String, bool>>>,
    pub main_model: Model,
    pub main_thinking_level: ThinkingLevel,
    pub cwd: PathBuf,
    /// Base directory for subagent session files. Subagent sessions land in
    /// `<session_dir>/<main_session_uuid>/subagent-N.jsonl`.
    pub session_dir: Option<PathBuf>,
    /// UUID of the main session — used as the per-session subdirectory.
    pub main_session_uuid: String,
    /// Path to the main session file (referenced as parent_session in subagent headers).
    pub main_session_file: Option<PathBuf>,
    /// Main agent's permission hook — subagents chain this after scope checks.
    pub permission_hook: Option<PreToolUseHook>,
}

pub struct SpawnConfig {
    pub name: Option<String>,
    pub system_prompt: String,
    pub model: Option<Model>,
    pub thinking_level: Option<ThinkingLevel>,
    pub scope: SubagentScope,
}

#[derive(Debug, Clone)]
pub struct SubagentSnapshot {
    pub info: SubagentInfo,
    pub messages: Vec<AgentMessage>,
}

pub struct SubagentManager {
    shared: SharedResources,
    runtimes: HashMap<String, SharedRuntime>,
    next_id: u64,
}

impl SubagentManager {
    pub fn new(shared: SharedResources) -> Self {
        Self {
            shared,
            runtimes: HashMap::new(),
            next_id: 1,
        }
    }

    /// Number of subagents currently in `Running` state.
    fn active_count(&self) -> usize {
        let mut n = 0;
        for rt in self.runtimes.values() {
            if let Ok(guard) = rt.try_lock() {
                if guard.info.status == SubagentStatus::Running {
                    n += 1;
                }
            } else {
                // If locked it's likely mid-send → count as active.
                n += 1;
            }
        }
        n
    }

    pub async fn spawn(&mut self, config: SpawnConfig) -> Result<SubagentInfo, String> {
        if self.active_count() >= MAX_ACTIVE_SUBAGENTS {
            return Err(format!(
                "too many active subagents (max {})",
                MAX_ACTIVE_SUBAGENTS
            ));
        }

        let id = format!("subagent-{}", self.next_id);
        self.next_id += 1;
        let name = config.name.unwrap_or_else(|| id.clone());

        let model = config
            .model
            .unwrap_or_else(|| self.shared.main_model.clone());
        let thinking_level = config
            .thinking_level
            .unwrap_or(self.shared.main_thinking_level);

        // Filter tools: exclude blocked, then apply scope whitelist.
        let main_tools_snapshot: Vec<Arc<dyn Tool>> = self.shared.main_tools.lock().await.clone();
        let tool_settings = self.shared.tool_settings.read().unwrap().clone();
        let filtered_tools: Vec<Arc<dyn Tool>> = main_tools_snapshot
            .into_iter()
            .filter(|tool| tool_settings.get(tool.name()).copied().unwrap_or(true))
            .filter(|t| !SUBAGENT_BLOCKED_TOOLS.contains(&t.name()))
            .filter(|t| match &config.scope.allowed_tools {
                AllowedTools::All => true,
                AllowedTools::Only(set) => set.contains(t.name()),
            })
            .collect();

        // Create session file if session_dir is configured.
        let (session_manager, session_file) = if let Some(base) = &self.shared.session_dir {
            let dir = base.join(&self.shared.main_session_uuid);
            let path = dir.join(format!("{}.jsonl", id));
            let session_uuid = uuid::Uuid::new_v4().to_string();
            let parent_session = self
                .shared
                .main_session_file
                .as_ref()
                .map(|p| p.to_string_lossy().to_string());
            match SessionManager::create(
                &path,
                session_uuid,
                self.shared.cwd.to_string_lossy().to_string(),
                parent_session,
            ) {
                Ok(mgr) => (Some(mgr), Some(path)),
                Err(e) => return Err(format!("failed to create subagent session: {}", e)),
            }
        } else {
            (None, None)
        };

        let now = current_timestamp_ms();
        let info = SubagentInfo {
            id: id.clone(),
            name,
            status: SubagentStatus::Idle,
            model_id: model.id.clone(),
            model_provider: model.provider.to_string(),
            thinking_level,
            created_at: now,
            last_activity_at: now,
            last_error: None,
            message_count: 0,
            session_file,
        };

        let (status_tx, _) = watch::channel(SubagentStatus::Idle);

        let runtime = SubagentRuntime {
            info: info.clone(),
            scope: config.scope,
            messages: Vec::new(),
            cancel_token: CancellationToken::new(),
            system_prompt: config.system_prompt,
            model,
            thinking_level,
            tools: filtered_tools,
            session_manager,
            status_tx,
        };

        self.runtimes
            .insert(id.clone(), Arc::new(Mutex::new(runtime)));

        Ok(info)
    }

    /// Deliver a user prompt to a subagent. Runs the agent_loop and persists results.
    /// If `wait` is true, blocks until the loop finishes.
    pub async fn send(&self, id: &str, text: &str, wait: bool) -> Result<(), String> {
        let rt_arc = self
            .runtimes
            .get(id)
            .ok_or_else(|| format!("subagent '{}' not found", id))?
            .clone();

        // Snapshot the values we need to build the agent_loop call, holding the lock briefly.
        let (context, loop_config, cancel_token, prompts) = {
            let mut rt = rt_arc.lock().await;

            if rt.info.status == SubagentStatus::Running {
                return Err(format!("subagent '{}' is already running", id));
            }
            if rt.info.status == SubagentStatus::Aborted {
                return Err(format!("subagent '{}' was aborted — create a new one", id));
            }

            // Reset cancel token if previously cancelled (e.g. after Done).
            if rt.cancel_token.is_cancelled() {
                rt.cancel_token = CancellationToken::new();
            }
            let cancel_token = rt.cancel_token.clone();

            let user_msg = AgentMessage::standard(Message::User(UserMessage {
                content: UserContent::Text(text.to_string()),
                display_text: None,
                timestamp: current_timestamp_ms(),
            }));
            rt.messages.push(user_msg.clone());

            // Persist user message to subagent session file.
            if let Some(mgr) = rt.session_manager.as_mut() {
                let _ = mgr.append_message(Message::User(UserMessage {
                    content: UserContent::Text(text.to_string()),
                    display_text: None,
                    timestamp: current_timestamp_ms(),
                }));
            }

            rt.info.status = SubagentStatus::Running;
            rt.info.last_activity_at = current_timestamp_ms();
            rt.info.last_error = None;
            rt.info.message_count = rt.messages.len();
            let _ = rt.status_tx.send(SubagentStatus::Running);

            let tool_schemas: Vec<ToolSchema> = rt
                .tools
                .iter()
                .map(|t| ToolSchema {
                    name: t.name().to_string(),
                    description: t.description().to_string(),
                    parameters: t.parameters_schema().clone(),
                })
                .collect();

            let context = AgentContext {
                system_prompt: Some(rt.system_prompt.clone()),
                messages: rt.messages.clone(),
                tools: tool_schemas,
            };

            let loop_config = build_loop_config(
                &self.shared,
                &rt.model,
                rt.thinking_level,
                rt.tools.clone(),
                rt.scope.clone(),
                self.shared.cwd.clone(),
            );

            (context, loop_config, cancel_token, vec![user_msg])
        };

        let stream = agent_loop(prompts, context, loop_config, Some(cancel_token));

        let rt_for_task = rt_arc.clone();
        let join = tokio::spawn(async move {
            drain_subagent_stream(stream, rt_for_task).await;
        });

        if wait {
            let _ = join.await;
        }
        Ok(())
    }

    /// Block until the subagent's status is no longer `Running`.
    pub async fn wait(&self, id: &str) -> Result<(), String> {
        let rt_arc = self
            .runtimes
            .get(id)
            .ok_or_else(|| format!("subagent '{}' not found", id))?
            .clone();

        let mut rx = {
            let rt = rt_arc.lock().await;
            rt.status_tx.subscribe()
        };

        loop {
            let status = *rx.borrow();
            if status != SubagentStatus::Running {
                return Ok(());
            }
            if rx.changed().await.is_err() {
                return Ok(());
            }
        }
    }

    pub async fn abort(&self, id: &str) -> Result<(), String> {
        let rt_arc = self
            .runtimes
            .get(id)
            .ok_or_else(|| format!("subagent '{}' not found", id))?
            .clone();

        let mut rt = rt_arc.lock().await;
        rt.cancel_token.cancel();
        rt.info.status = SubagentStatus::Aborted;
        rt.info.last_activity_at = current_timestamp_ms();
        let _ = rt.status_tx.send(SubagentStatus::Aborted);
        Ok(())
    }

    pub async fn list(&self) -> Vec<SubagentInfo> {
        let mut out = Vec::with_capacity(self.runtimes.len());
        for rt in self.runtimes.values() {
            let guard = rt.lock().await;
            out.push(guard.info.clone());
        }
        out
    }

    /// Return the effective tools captured by a spawned subagent.
    pub async fn tool_names(&self, id: &str) -> Option<Vec<String>> {
        let runtime = self.runtimes.get(id)?;
        let runtime = runtime.lock().await;
        Some(
            runtime
                .tools
                .iter()
                .map(|tool| tool.name().to_owned())
                .collect(),
        )
    }

    /// Synchronous best-effort listing — skips runtimes whose lock is currently held.
    /// Used by UI rendering paths that must not block.
    pub fn list_sync(&self) -> Vec<SubagentInfo> {
        let mut out = Vec::with_capacity(self.runtimes.len());
        for rt in self.runtimes.values() {
            if let Ok(guard) = rt.try_lock() {
                out.push(guard.info.clone());
            }
        }
        out
    }

    pub async fn get_messages(&self, id: &str) -> Option<Vec<AgentMessage>> {
        let rt = self.runtimes.get(id)?.clone();
        let guard = rt.lock().await;
        Some(guard.messages.clone())
    }

    pub async fn snapshot(&self, id: &str) -> Option<SubagentSnapshot> {
        let rt = self.runtimes.get(id)?.clone();
        let guard = rt.lock().await;
        Some(SubagentSnapshot {
            info: guard.info.clone(),
            messages: guard.messages.clone(),
        })
    }
}

fn build_loop_config(
    shared: &SharedResources,
    model: &Model,
    thinking_level: ThinkingLevel,
    tools: Vec<Arc<dyn Tool>>,
    scope: SubagentScope,
    cwd: PathBuf,
) -> AgentLoopConfig {
    let reasoning = match thinking_level {
        ThinkingLevel::Off => None,
        level => Some(level),
    };

    let stream_options = SimpleStreamOptions {
        base: StreamOptions {
            temperature: None,
            max_tokens: Some(model.max_tokens),
            api_key: resolve_api_key(model),
            transport: Transport::Auto,
            cache_retention: CacheRetention::Short,
            session_id: None,
            headers: model.headers.clone(),
            timeout_ms: None,
            max_retries: Some(2),
            max_retry_delay_ms: None,
            metadata: None,
        },
        reasoning,
        thinking_budgets: None,
        tool_choice: None,
    };

    let model_stream_arc = shared.model_stream.clone();
    let model_stream: ModelStreamFn = Box::new(move |m, c, o| model_stream_arc(m, c, o));

    let convert_arc = shared.convert_to_llm.clone();
    let convert_to_llm: Box<dyn Fn(&[AgentMessage]) -> Vec<Message> + Send + Sync> =
        Box::new(move |msgs| convert_arc(msgs));

    let pre_tool_use: Option<
        Box<
            dyn Fn(
                    PreToolUseContext,
                ) -> std::pin::Pin<
                    Box<dyn std::future::Future<Output = Option<PreToolUseResult>> + Send>,
                > + Send
                + Sync,
        >,
    > = {
        let scope = scope.clone();
        let cwd = cwd.clone();
        let permission_hook = shared.permission_hook.clone();
        Some(Box::new(move |ctx: PreToolUseContext| {
            let scope = scope.clone();
            let cwd = cwd.clone();
            let permission_hook = permission_hook.clone();
            Box::pin(async move {
                // 1. Scope check first (subagent-specific restrictions).
                if let Err(reason) = scope.check_tool_allowed(&ctx.tool_name, &ctx.args, &cwd) {
                    return Some(PreToolUseResult {
                        block: true,
                        reason: Some(reason),
                    });
                }
                // 2. Then run main permission policy (blacklist + user approval).
                if let Some(hook) = permission_hook {
                    return hook(ctx).await;
                }
                None
            })
        }))
    };

    AgentLoopConfig {
        model: model.clone(),
        reasoning,
        stream_options,
        model_stream,
        convert_to_llm,
        transform_context: None,
        get_api_key: None,
        should_stop_after_turn: None,
        prepare_next_turn: None,
        get_steering_messages: None,
        get_follow_up_messages: None,
        max_turns: Some(100),
        tool_execution: ToolExecutionMode::Parallel,
        pre_tool_use,
        post_tool_use: None,
        tools,
    }
}

async fn drain_subagent_stream(
    mut stream: rozsa_model::event_stream::EventStream<AgentEvent>,
    rt_arc: SharedRuntime,
) {
    let had_error = false;
    while let Some(event) = stream.next().await {
        match &event {
            AgentEvent::MessageEnd { message } => {
                let mut rt = rt_arc.lock().await;
                rt.messages.push(message.clone());
                rt.info.message_count = rt.messages.len();
                rt.info.last_activity_at = current_timestamp_ms();
                if let Some(mgr) = rt.session_manager.as_mut() {
                    if let Some(std_msg) = message.as_standard() {
                        let _ = mgr.append_message(std_msg.clone());
                    }
                }
            }
            AgentEvent::AgentEnd { .. } => {
                let mut rt = rt_arc.lock().await;
                rt.info.last_activity_at = current_timestamp_ms();
                // If we were aborted, keep the Aborted status.
                if rt.info.status == SubagentStatus::Running {
                    rt.info.status = if had_error {
                        SubagentStatus::Error
                    } else {
                        SubagentStatus::Idle
                    };
                    let _ = rt.status_tx.send(rt.info.status);
                }
            }
            _ => {}
        }
        // No "Error" event in AgentEvent enum yet — toolresult is_error gets tracked elsewhere.
        let _ = had_error;
    }

    // Stream finished without AgentEnd (cancellation) — ensure status leaves Running.
    let mut rt = rt_arc.lock().await;
    if rt.info.status == SubagentStatus::Running {
        rt.info.status = SubagentStatus::Idle;
        let _ = rt.status_tx.send(SubagentStatus::Idle);
    }
}

fn resolve_api_key(model: &Model) -> Option<String> {
    use rozsa_model::types::Provider;
    let env_var = match &model.provider {
        Provider::Anthropic => "ANTHROPIC_API_KEY",
        Provider::OpenAI => "OPENAI_API_KEY",
        Provider::Google | Provider::GoogleVertex => "GOOGLE_API_KEY",
        Provider::DeepSeek => "DEEPSEEK_API_KEY",
        Provider::OpenRouter => "OPENROUTER_API_KEY",
        Provider::XAI => "XAI_API_KEY",
        Provider::Groq => "GROQ_API_KEY",
        Provider::Mistral => "MISTRAL_API_KEY",
        Provider::Together => "TOGETHER_API_KEY",
        Provider::HuggingFace => "HF_TOKEN",
        Provider::Custom(_) => return std::env::var("LLM_API_KEY").ok(),
        _ => return None,
    };
    std::env::var(env_var).ok()
}

fn current_timestamp_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
