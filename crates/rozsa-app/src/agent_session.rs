// File: agent_session.rs
//
// Internal Framework:
// agent_session.rs
// ├── AgentSessionConfig        # Configuration bundle for session creation
// ├── AgentSession              # Top-level orchestrator
// │   ├── new()                 # Create from config
// │   ├── register_tool()       # Add a single tool
// │   ├── register_default_tools()  # Register read/write/edit/bash/ls/grep/find
// │   ├── prompt()              # Send user message and run agent loop
// │   ├── continue_session()    # Continue without new user input
// │   └── abort()               # Cancel the running loop
// └── Helper functions
//     ├── build_agent_context() # Build AgentContext from session state
//     ├── build_loop_config()   # Build AgentLoopConfig with all hooks wired
//     ├── convert_to_llm()      # Filter messages for LLM consumption
//     └── collect_events()      # Drain EventStream into Vec
//
// Related Docs:
// - [Session Manager](./session/manager.rs)
// - [Settings](./settings/mod.rs)
// - [Tools](./tools/mod.rs)
// - [Core Agent Loop](../../rozsa-core/src/agent_loop.rs)

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::Result;
use tokio::sync::{Mutex, broadcast};
use tokio_util::sync::CancellationToken;

use rozsa_core::agent_loop::{agent_loop, agent_loop_continue};
use rozsa_core::config::{AgentContext, AgentLoopConfig, ModelStreamFn, ShouldStopContext};
use rozsa_core::events::AgentEvent;
use rozsa_core::messages::AgentMessage;
use rozsa_core::tool::{Tool, ToolExecutionMode};
use rozsa_model::event_stream::EventStream;
use rozsa_model::types::{
    CacheRetention, ContentBlock, Message, Model, SimpleStreamOptions, StreamOptions,
    ThinkingLevel, ToolSchema, Transport, UserContent, UserMessage,
};

use crate::resources::LoadedResources;
use crate::session::manager::SessionManager;
use crate::settings::SettingsManager;
use crate::tools::{
    create_bash_tool, create_edit_tool, create_find_tool, create_grep_tool, create_ls_tool,
    create_read_tool, create_subagent_tool, create_write_tool,
};

/// Configuration bundle for creating an AgentSession.
///
/// Aggregates all dependencies the session orchestrator needs: model selection,
/// system prompt, persistence, settings, and extension hooks.
pub struct AgentSessionConfig {
    /// Model to use for LLM requests.
    pub model: Model,
    /// Thinking/reasoning level for the model.
    pub thinking_level: ThinkingLevel,
    /// System prompt text (assembled from resources).
    pub system_prompt: String,
    /// Working directory for tool execution.
    pub cwd: PathBuf,
    /// Session persistence manager.
    pub session_manager: SessionManager,
    /// Settings manager (resolved settings).
    pub settings_manager: SettingsManager,
    /// Loaded resources (CLAUDE.md, AGENTS.md, etc.).
    pub resources: LoadedResources,
    /// Optional pre-tool-use hook for permission checking.
    pub pre_tool_use: Option<
        Box<
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

/// Static (immutable per session) parts of AgentSessionConfig.
struct StaticConfig {
    system_prompt: String,
    cwd: PathBuf,
    settings_manager: SettingsManager,
    #[allow(dead_code)]
    resources: LoadedResources,
}

/// Mutable runtime parameters: model and reasoning level can change between turns.
struct RuntimeParams {
    model: Model,
    thinking_level: ThinkingLevel,
}

/// Top-level orchestrator that wires together the agent loop, tools,
/// permissions, extensions, and session persistence.
///
/// This is the main entry point for running a conversation turn.
/// It owns the tools, manages cancellation, and delegates to
/// `rozsa_core::agent_loop` for the actual model interaction loop.
pub struct AgentSession {
    static_config: StaticConfig,
    runtime: Mutex<RuntimeParams>,
    session_manager: Mutex<SessionManager>,
    tools: Arc<Mutex<Vec<Arc<dyn Tool>>>>,
    cancel_token: Mutex<Option<CancellationToken>>,
    is_running: AtomicBool,
    /// Accumulated messages across turns (the conversation history).
    messages: Mutex<Vec<AgentMessage>>,
    /// Broadcast channel for AgentEvents — subscribers see every event the loop emits.
    event_tx: broadcast::Sender<AgentEvent>,
    /// Whether compaction is in progress.
    is_compacting: AtomicBool,
    /// Runtime state: edit mode, tool stats, permission mode.
    runtime_state: Arc<tokio::sync::Mutex<crate::runtime_state::RuntimeState>>,
    /// Steering message queue — delivered between tool calls.
    steering_queue: Arc<std::sync::Mutex<Vec<AgentMessage>>>,
    /// Follow-up message queue — delivered when no steering/tool calls remain.
    follow_up_queue: Arc<std::sync::Mutex<Vec<AgentMessage>>>,
    /// Optional pre-tool-use hook (injected by backend for permissions).
    pre_tool_use_hook: Option<
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
    /// Extension lifecycle hooks.
    extension_runner: tokio::sync::Mutex<crate::extensions::ExtensionRunner>,
    /// Skill registry — loaded from filesystem at startup, reloadable.
    skill_registry: std::sync::RwLock<crate::skills::SkillRegistry>,
    /// Subagent manager — owns spawned subagents (lazy init on first access).
    subagent_manager: Arc<tokio::sync::Mutex<crate::subagent::SubagentManager>>,
    /// Currently-viewed subagent in the UI (None = main session).
    viewing_subagent_id: tokio::sync::Mutex<Option<String>>,
}

impl AgentSession {
    /// Create a new agent session from configuration.
    pub fn new(config: AgentSessionConfig) -> Self {
        // Capacity 2048: headroom for token-stream bursts and tool update events.
        let (event_tx, _) = broadcast::channel(2048);
        let AgentSessionConfig {
            model,
            thinking_level,
            system_prompt,
            cwd,
            session_manager,
            settings_manager,
            resources,
            pre_tool_use,
        } = config;
        let permission_mode = settings_manager.resolved().permissions.mode.clone();
        let skill_registry = crate::skills::SkillRegistry::load_from_defaults(&cwd);

        let tools_arc: Arc<Mutex<Vec<Arc<dyn Tool>>>> = Arc::new(Mutex::new(Vec::new()));
        let main_session_uuid = session_manager.session_id().to_string();
        let main_session_file = Some(session_manager.session_file().to_path_buf());
        let session_dir = main_session_file
            .as_ref()
            .and_then(|p| p.parent().map(|d| d.to_path_buf()));
        // Arc-wrap the permission hook early so it can be shared with subagents.
        let pre_tool_use_arc: Option<
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
        > = pre_tool_use.map(|f| Arc::from(f) as _);

        let shared = crate::subagent::SharedResources {
            model_stream: Arc::new(|m, c, o| rozsa_model::stream::stream_simple(m, c, o)),
            convert_to_llm: Arc::new(convert_to_llm),
            main_tools: tools_arc.clone(),
            main_model: model.clone(),
            main_thinking_level: thinking_level,
            cwd: cwd.clone(),
            session_dir,
            main_session_uuid,
            main_session_file,
            permission_hook: pre_tool_use_arc.clone(),
        };
        let subagent_manager = Arc::new(tokio::sync::Mutex::new(
            crate::subagent::SubagentManager::new(shared),
        ));

        Self {
            static_config: StaticConfig {
                system_prompt,
                cwd,
                settings_manager,
                resources,
            },
            runtime: Mutex::new(RuntimeParams {
                model,
                thinking_level,
            }),
            session_manager: Mutex::new(session_manager),
            tools: tools_arc,
            cancel_token: Mutex::new(None),
            is_running: AtomicBool::new(false),
            messages: Mutex::new(Vec::new()),
            event_tx,
            is_compacting: AtomicBool::new(false),
            runtime_state: Arc::new(tokio::sync::Mutex::new(
                crate::runtime_state::RuntimeState::new(&permission_mode),
            )),
            steering_queue: Arc::new(std::sync::Mutex::new(Vec::new())),
            follow_up_queue: Arc::new(std::sync::Mutex::new(Vec::new())),
            pre_tool_use_hook: pre_tool_use_arc,
            extension_runner: tokio::sync::Mutex::new(crate::extensions::ExtensionRunner::new()),
            skill_registry: std::sync::RwLock::new(skill_registry),
            subagent_manager,
            viewing_subagent_id: tokio::sync::Mutex::new(None),
        }
    }

    /// Register an extension that receives lifecycle hooks.
    pub async fn register_extension(&self, extension: Box<dyn crate::extensions::Extension>) {
        self.extension_runner.lock().await.register(extension);
    }

    /// Access the skill registry (read lock).
    pub fn skill_registry(&self) -> std::sync::RwLockReadGuard<'_, crate::skills::SkillRegistry> {
        self.skill_registry.read().unwrap()
    }

    /// Reload skills from filesystem.
    /// Returns diagnostics for skills that failed to load.
    pub fn reload_skills(&self) -> Vec<crate::skills::loader::SkillDiagnostic> {
        let cwd = &self.static_config.cwd;
        let (new_registry, diagnostics) =
            crate::skills::SkillRegistry::load_from_defaults_with_diagnostics(cwd);
        *self.skill_registry.write().unwrap() = new_registry;
        diagnostics
    }

    /// Subscribe to AgentEvents emitted by `prompt` / `continue_session`.
    /// Each subscriber sees every event from the moment of subscription onward.
    pub fn subscribe(&self) -> broadcast::Receiver<AgentEvent> {
        self.event_tx.subscribe()
    }

    /// Register a single tool.
    pub async fn register_tool(&self, tool: Arc<dyn Tool>) {
        self.tools.lock().await.push(tool);
    }

    /// Register the default built-in tools (read, write, edit, bash, ls, grep, find, subagent).
    pub async fn register_default_tools(&self, cwd: &Path) {
        let cwd_str = cwd.to_string_lossy().to_string();
        let defaults: Vec<Box<dyn Tool>> = vec![
            create_read_tool(),
            create_write_tool(),
            create_edit_tool(),
            create_bash_tool(cwd_str),
            create_ls_tool(),
            create_grep_tool(),
            create_find_tool(),
            create_subagent_tool(self.subagent_manager.clone()),
        ];
        let mut tools = self.tools.lock().await;
        for tool in defaults {
            tools.push(Arc::from(tool));
        }
    }

    /// Send a user message and run the agent loop to completion.
    ///
    /// Builds the agent context from current session state, constructs a user
    /// message, runs the core loop, persists resulting messages, and returns
    /// the event stream collected as a Vec.
    pub async fn prompt(&self, message: &str) -> Result<Vec<AgentEvent>> {
        self.prompt_with_prefix_blocks(message, Vec::new(), None)
            .await
    }

    /// Send a user message with extra leading content blocks.
    ///
    /// GUI file mentions use this to keep the visible user text unchanged via
    /// `display_text` while sending expanded file/image context to the model.
    pub async fn prompt_with_prefix_blocks(
        &self,
        message: &str,
        mut prefix_blocks: Vec<ContentBlock>,
        display_text_override: Option<String>,
    ) -> Result<Vec<AgentEvent>> {
        // Atomically claim the running slot — concurrent submits get rejected.
        if self
            .is_running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            anyhow::bail!("Agent session is already running");
        }

        let cancel_token = CancellationToken::new();
        *self.cancel_token.lock().await = Some(cancel_token.clone());

        // Expand /skill:name commands
        let (expanded_text, display_text) = self.expand_skill_command(message);
        let display_text = display_text_override.or(display_text);
        let content = if prefix_blocks.is_empty() {
            UserContent::Text(expanded_text.clone())
        } else {
            prefix_blocks.push(ContentBlock::Text {
                text: expanded_text.clone(),
                signature: None,
            });
            UserContent::Blocks(prefix_blocks.clone())
        };

        // Build user message
        let user_msg = AgentMessage::standard(Message::User(UserMessage {
            content: content.clone(),
            display_text: display_text.clone(),
            timestamp: current_timestamp_ms(),
        }));

        // Persist user message to session file
        self.session_manager
            .lock()
            .await
            .append_message(Message::User(UserMessage {
                content,
                display_text,
                timestamp: current_timestamp_ms(),
            }))?;

        let context = self.build_agent_context().await;
        let loop_config = match self.build_loop_config().await {
            Ok(config) => config,
            Err(error) => {
                *self.cancel_token.lock().await = None;
                self.is_running.store(false, Ordering::SeqCst);
                return Err(error);
            }
        };

        let stream = agent_loop(vec![user_msg], context, loop_config, Some(cancel_token));

        let events = self.drain_and_broadcast(stream).await;
        self.persist_new_messages(&events).await?;

        *self.cancel_token.lock().await = None;
        self.is_running.store(false, Ordering::SeqCst);

        Ok(events)
    }

    /// Continue the session without a new user message.
    ///
    /// Used after interruptions or when the model needs to continue
    /// processing (e.g., after compaction).
    pub async fn continue_session(&self) -> Result<Vec<AgentEvent>> {
        if self
            .is_running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            anyhow::bail!("Agent session is already running");
        }

        let cancel_token = CancellationToken::new();
        *self.cancel_token.lock().await = Some(cancel_token.clone());

        let context = self.build_agent_context().await;
        let loop_config = match self.build_loop_config().await {
            Ok(config) => config,
            Err(error) => {
                *self.cancel_token.lock().await = None;
                self.is_running.store(false, Ordering::SeqCst);
                return Err(error);
            }
        };

        let stream = agent_loop_continue(context, loop_config, Some(cancel_token));

        let events = self.drain_and_broadcast(stream).await;
        self.persist_new_messages(&events).await?;

        *self.cancel_token.lock().await = None;
        self.is_running.store(false, Ordering::SeqCst);

        Ok(events)
    }

    /// Drain an EventStream while fan-out broadcasting each event to subscribers.
    /// Returns the same Vec the old `collect_events` produced — callers see no behavior change.
    async fn drain_and_broadcast(&self, mut stream: EventStream<AgentEvent>) -> Vec<AgentEvent> {
        let mut events = Vec::new();
        while let Some(event) = stream.next().await {
            // send() errors only when there are zero subscribers — not an error condition.
            let _ = self.event_tx.send(event.clone());
            events.push(event);
        }
        events
    }

    /// Abort the currently running loop.
    ///
    /// Signals the CancellationToken, causing the agent loop to terminate
    /// gracefully at the next check point. Safe to call concurrently with `prompt`.
    pub async fn abort(&self) {
        if let Some(token) = self.cancel_token.lock().await.as_ref() {
            token.cancel();
        }
    }

    /// Whether the session is currently running an agent loop.
    pub fn is_running(&self) -> bool {
        self.is_running.load(Ordering::SeqCst)
    }

    /// Snapshot the current accumulated messages.
    pub async fn messages(&self) -> Vec<AgentMessage> {
        self.messages.lock().await.clone()
    }

    /// Access the subagent manager (lock-guarded).
    pub async fn subagent_manager(
        &self,
    ) -> tokio::sync::MutexGuard<'_, crate::subagent::SubagentManager> {
        self.subagent_manager.lock().await
    }

    /// Non-blocking access to the subagent manager — returns None if locked.
    /// Used by the synchronous render path to avoid blocking the UI thread.
    pub fn subagent_manager_try_lock(
        &self,
    ) -> Option<tokio::sync::MutexGuard<'_, crate::subagent::SubagentManager>> {
        self.subagent_manager.try_lock().ok()
    }

    /// Get the ID of the subagent currently being viewed (None = main session).
    pub async fn viewing_subagent_id(&self) -> Option<String> {
        self.viewing_subagent_id.lock().await.clone()
    }

    /// Non-blocking read of the viewing-subagent id. Returns the inner Option<String>
    /// on successful try_lock; returns None when the lock is held (treated as
    /// "no view information available right now").
    pub fn viewing_subagent_id_try_lock(&self) -> Option<String> {
        self.viewing_subagent_id
            .try_lock()
            .ok()
            .and_then(|g| g.clone())
    }

    /// Set the subagent currently being viewed.
    pub async fn set_viewing_subagent(&self, id: Option<String>) {
        *self.viewing_subagent_id.lock().await = id;
    }

    /// Lock and access the session manager. Holds the lock for the duration of the borrow —
    /// keep usage short to avoid blocking concurrent operations.
    pub async fn session_manager(&self) -> tokio::sync::MutexGuard<'_, SessionManager> {
        self.session_manager.lock().await
    }

    /// Switch to a different session file. Replaces the internal SessionManager
    /// and clears conversation history. Returns the old session path.
    pub async fn switch_session(
        &self,
        path: impl AsRef<std::path::Path>,
    ) -> anyhow::Result<String> {
        let new_mgr = SessionManager::open(&path)?;
        let mut mgr = self.session_manager.lock().await;
        let old_path = mgr.session_file().to_string_lossy().to_string();
        *mgr = new_mgr;
        drop(mgr);
        // Clear in-memory conversation — the new session has its own history.
        self.messages.lock().await.clear();
        Ok(old_path)
    }

    /// Get the settings manager (read-only, immutable for the session lifetime).
    pub fn settings_manager(&self) -> &SettingsManager {
        &self.static_config.settings_manager
    }

    /// Get the working directory.
    pub fn cwd(&self) -> &Path {
        &self.static_config.cwd
    }

    /// Get the current thinking level.
    pub async fn thinking_level(&self) -> ThinkingLevel {
        self.runtime.lock().await.thinking_level
    }

    /// Get the current model (cloned snapshot).
    pub async fn model(&self) -> Model {
        self.runtime.lock().await.model.clone()
    }

    /// Update the model for subsequent turns.
    pub async fn set_model(&self, model: Model) {
        self.runtime.lock().await.model = model;
    }

    /// Update the thinking level for subsequent turns.
    pub async fn set_thinking_level(&self, level: ThinkingLevel) {
        self.runtime.lock().await.thinking_level = level;
    }

    // --- Phase A: State accessors ---

    /// Get show_images setting.
    pub fn show_images(&self) -> bool {
        !self.static_config.settings_manager.resolved().block_images
    }

    /// Get hide_thinking flag (true when thinking is off).
    pub async fn hide_thinking(&self) -> bool {
        self.runtime.lock().await.thinking_level == ThinkingLevel::Off
    }

    // --- Phase B: Compaction ---

    /// Whether compaction is currently running.
    pub fn is_compacting(&self) -> bool {
        self.is_compacting.load(Ordering::SeqCst)
    }

    /// Run compaction: abort loop, summarize old messages, replace history.
    pub async fn compact(&self) -> Result<crate::compaction::CompactionResult> {
        use crate::compaction::{CompactionEngine, CompactionTrigger};

        self.is_compacting.store(true, Ordering::SeqCst);

        // Abort running loop if any
        if self.is_running() {
            self.abort().await;
            // Give the loop time to stop
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }

        let settings = self.static_config.settings_manager.resolved().clone();
        let engine = CompactionEngine::new(CompactionTrigger {
            threshold_tokens: settings.compaction.threshold_tokens,
            target_tokens: settings.compaction.target_tokens,
        });

        let entries = self.session_manager.lock().await.entries();
        let plan = engine.prepare(&entries);

        let Some(plan) = plan else {
            self.is_compacting.store(false, Ordering::SeqCst);
            anyhow::bail!("Nothing to compact — token usage below threshold");
        };

        // Build summarize function using the current model
        let runtime = self.runtime.lock().await;
        let model = runtime.model.clone();
        let thinking_level = runtime.thinking_level;
        drop(runtime);

        let credentials = resolve_credentials(&model).await?;
        let summarize_fn = |content: String| {
            let model = model.clone();
            let api_key = credentials.api_key.clone();
            let headers = merge_headers(model.headers.clone(), credentials.headers.clone());
            async move {
                let prompt = format!(
                    "Summarize the following conversation history concisely, \
                     preserving key decisions, code changes, and context needed to continue:\n\n{}",
                    content
                );
                let context = rozsa_model::types::Context {
                    system_prompt: Some(
                        "You are a conversation summarizer. Produce a concise summary.".to_string(),
                    ),
                    messages: vec![Message::User(UserMessage {
                        content: UserContent::Text(prompt),
                        display_text: None,
                        timestamp: current_timestamp_ms(),
                    })],
                    tools: vec![],
                };
                let reasoning = match thinking_level {
                    ThinkingLevel::Off => None,
                    level => Some(level),
                };
                let options = SimpleStreamOptions {
                    base: StreamOptions {
                        temperature: None,
                        max_tokens: Some(4096),
                        api_key,
                        transport: Transport::Auto,
                        cache_retention: CacheRetention::Short,
                        session_id: None,
                        headers,
                        timeout_ms: None,
                        max_retries: Some(2),
                        max_retry_delay_ms: None,
                        metadata: None,
                    },
                    reasoning,
                    thinking_budgets: None,
                    tool_choice: None,
                };
                let mut stream = rozsa_model::stream::stream_simple(&model, &context, &options);
                let mut result_text = String::new();
                while let Some(event) = stream.next().await {
                    if let rozsa_model::types::StreamEvent::Done { message, .. } = event {
                        for block in &message.content {
                            if let rozsa_model::types::ContentBlock::Text { text, .. } = block {
                                result_text.push_str(text);
                            }
                        }
                        break;
                    }
                }
                if result_text.is_empty() {
                    anyhow::bail!("Compaction summarization returned empty result");
                }
                Ok(result_text)
            }
        };

        let result = engine.execute(&plan, &entries, summarize_fn).await?;

        // Persist compaction entry
        let first_kept_id = if plan.cut_point_index < entries.len() {
            entries[plan.cut_point_index].id().to_string()
        } else {
            String::new()
        };
        self.session_manager.lock().await.append_compaction(
            result.summary.clone(),
            first_kept_id,
            plan.estimated_tokens_before,
            None,
            None,
        )?;

        // Rebuild messages: keep only messages from cut_point_index onwards
        // plus prepend a summary message
        let mut messages = self.messages.lock().await;
        let kept_count = messages.len().saturating_sub(result.removed_count);
        let kept_messages: Vec<AgentMessage> =
            messages.iter().rev().take(kept_count).cloned().collect();
        let summary_msg = AgentMessage::custom(
            "compaction_summary".to_string(),
            serde_json::json!({ "summary": &result.summary }),
            current_timestamp_ms(),
        );
        *messages = std::iter::once(summary_msg)
            .chain(kept_messages.into_iter().rev())
            .collect();
        drop(messages);

        self.is_compacting.store(false, Ordering::SeqCst);
        Ok(result)
    }

    // --- Phase D: Runtime state ---

    /// Get a serializable snapshot of runtime state.
    pub async fn runtime_state_snapshot(&self) -> crate::runtime_state::RuntimeStateSnapshot {
        self.runtime_state.lock().await.snapshot()
    }

    /// Cycle edit mode and return the new mode.
    pub async fn cycle_edit_mode(&self) -> crate::runtime_state::EditMode {
        let mut state = self.runtime_state.lock().await;
        state.edit_mode = state.edit_mode.cycle();
        state.edit_mode
    }

    // --- Phase E: Queues ---

    /// Enqueue a steering message (delivered between tool calls).
    pub fn steer(&self, text: &str) {
        let msg = AgentMessage::standard(Message::User(UserMessage {
            content: UserContent::Text(text.to_string()),
            display_text: Some(format!("[steer] {}", text)),
            timestamp: current_timestamp_ms(),
        }));
        self.steering_queue.lock().unwrap().push(msg);
    }

    /// Enqueue a follow-up message (delivered after all tools/steering done).
    pub fn follow_up(&self, text: &str) {
        let msg = AgentMessage::standard(Message::User(UserMessage {
            content: UserContent::Text(text.to_string()),
            display_text: Some(format!("[follow-up] {}", text)),
            timestamp: current_timestamp_ms(),
        }));
        self.follow_up_queue.lock().unwrap().push(msg);
    }

    /// Get pending message descriptions for UI display.
    pub fn pending_messages(&self) -> Vec<String> {
        let mut pending = Vec::new();
        for msg in self.steering_queue.lock().unwrap().iter() {
            if let Some(Message::User(u)) = msg.as_standard() {
                let t = u.content.text();
                if !t.is_empty() {
                    pending.push(format!("[steer] {}", t));
                }
            }
        }
        for msg in self.follow_up_queue.lock().unwrap().iter() {
            if let Some(Message::User(u)) = msg.as_standard() {
                let t = u.content.text();
                if !t.is_empty() {
                    pending.push(format!("[follow-up] {}", t));
                }
            }
        }
        pending
    }

    /// Execute a bash command directly (not through agent loop).
    pub async fn execute_bash(&self, command: &str) -> Result<String> {
        let tools = self.tools.lock().await;
        let bash_tool = tools.iter().find(|t| t.name() == "Bash");
        let Some(tool) = bash_tool.cloned() else {
            anyhow::bail!("Bash tool not registered");
        };
        drop(tools);

        let args = serde_json::json!({ "command": command });
        let result = tool
            .execute("direct-bash", args, None, None)
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))?;

        let output: String = result
            .content
            .iter()
            .filter_map(|b| match b {
                rozsa_model::types::ContentBlock::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");

        // Record as custom message in history
        let msg = AgentMessage::custom(
            "bash_execution".to_string(),
            serde_json::json!({ "command": command, "output": &output }),
            current_timestamp_ms(),
        );
        self.messages.lock().await.push(msg);

        Ok(output)
    }

    // --- Private helpers ---

    /// Expand `/skill:name [args]` into XML block, returning (expanded_text, display_text).
    /// If not a skill command or skill not found, returns (original, None).
    fn expand_skill_command(&self, text: &str) -> (String, Option<String>) {
        if !text.starts_with("/skill:") {
            return (text.to_string(), None);
        }

        let space_idx = text.find(' ');
        let skill_name = match space_idx {
            Some(idx) => &text[7..idx],
            None => &text[7..],
        };
        let args = space_idx.map(|idx| text[idx + 1..].trim()).unwrap_or("");

        let registry = self.skill_registry.read().unwrap();
        let Some(skill) = registry.find_by_name(skill_name) else {
            return (text.to_string(), None);
        };

        let content = match std::fs::read_to_string(&skill.file_path) {
            Ok(c) => c,
            Err(_) => return (text.to_string(), None),
        };

        let body = crate::skills::loader::strip_frontmatter(&content).trim();
        let base_dir = skill.base_dir.display();
        let mut expanded = format!(
            "<skill>\n<name>{}</name>\n<content>\n{}\n</content>\n<base_dir>{}</base_dir>\n</skill>",
            skill_name, body, base_dir
        );

        if !args.is_empty() {
            expanded.push_str("\n\n");
            expanded.push_str(args);
        }

        (expanded, Some(text.to_string()))
    }

    /// Build an AgentContext from the current session state.
    async fn build_agent_context(&self) -> AgentContext {
        let tool_schemas: Vec<ToolSchema> = self
            .tools
            .lock()
            .await
            .iter()
            .map(|t| ToolSchema {
                name: t.name().to_string(),
                description: t.description().to_string(),
                parameters: t.parameters_schema().clone(),
            })
            .collect();

        let mut system_prompt = self.static_config.system_prompt.clone();
        let skill_fragment = self.skill_registry.read().unwrap().format_for_prompt();
        if !skill_fragment.is_empty() {
            system_prompt.push_str("\n\n");
            system_prompt.push_str(&skill_fragment);
        }

        AgentContext {
            system_prompt: Some(system_prompt),
            messages: self.messages.lock().await.clone(),
            tools: tool_schemas,
        }
    }

    /// Build the AgentLoopConfig wiring all hooks to this session's dependencies.
    async fn build_loop_config(&self) -> Result<AgentLoopConfig> {
        let runtime = self.runtime.lock().await;
        let model = runtime.model.clone();
        let thinking_level = runtime.thinking_level;
        drop(runtime);
        let settings = self.static_config.settings_manager.resolved().clone();

        // Build stream options from settings
        let reasoning = match thinking_level {
            ThinkingLevel::Off => None,
            level => Some(level),
        };

        let credentials = resolve_credentials(&model).await?;

        let stream_options = SimpleStreamOptions {
            base: StreamOptions {
                temperature: None,
                max_tokens: Some(model.max_tokens),
                api_key: credentials.api_key,
                transport: Transport::Auto,
                cache_retention: CacheRetention::Short,
                session_id: Some(self.session_manager.lock().await.session_id().to_string()),
                headers: merge_headers(model.headers.clone(), credentials.headers),
                timeout_ms: settings.retry.timeout_ms,
                max_retries: settings.retry.max_retries,
                max_retry_delay_ms: settings.retry.max_retry_delay_ms,
                metadata: None,
            },
            reasoning,
            thinking_budgets: None,
            tool_choice: None,
        };

        // Compaction threshold from settings
        let compaction_threshold = settings.compaction.threshold_tokens;
        let compaction_enabled = settings.compaction.enabled;

        // Clone queues and state for closure capture
        let steering_q = self.steering_queue.clone();
        let follow_up_q = self.follow_up_queue.clone();
        let runtime_state_for_post = self.runtime_state.clone();

        Ok(AgentLoopConfig {
            model: model.clone(),
            reasoning,
            stream_options,
            model_stream: build_model_stream_fn(),
            convert_to_llm: Box::new(convert_to_llm),
            transform_context: None,
            get_api_key: None,
            should_stop_after_turn: if compaction_enabled {
                Some(Box::new(move |ctx: &ShouldStopContext| {
                    should_stop_for_compaction(ctx, compaction_threshold)
                }))
            } else {
                None
            },
            prepare_next_turn: None,
            get_steering_messages: Some(Box::new(move || {
                std::mem::take(&mut *steering_q.lock().unwrap())
            })),
            get_follow_up_messages: Some(Box::new(move || {
                std::mem::take(&mut *follow_up_q.lock().unwrap())
            })),
            max_turns: Some(200),
            tool_execution: ToolExecutionMode::Parallel,
            pre_tool_use: {
                let runtime_state_for_pre = self.runtime_state.clone();
                let external_hook = self.pre_tool_use_hook.clone();
                let boxed: Box<
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
                > = Box::new(move |ctx| {
                    let rs = runtime_state_for_pre.clone();
                    let hook = external_hook.clone();
                    Box::pin(async move {
                        // Edit mode gate: block tools when in think_first mode.
                        if let Ok(state) = rs.try_lock() {
                            if let Some(reason) = state
                                .edit_mode
                                .check_tool_blocked(&ctx.tool_name, &ctx.args)
                            {
                                return Some(rozsa_core::config::PreToolUseResult {
                                    block: true,
                                    reason: Some(reason),
                                });
                            }
                        }
                        // Then run external permission hook if present.
                        if let Some(ref h) = hook {
                            return h(ctx).await;
                        }
                        None
                    })
                });
                Some(boxed)
            },
            post_tool_use: {
                let rs = runtime_state_for_post;
                Some(Box::new(
                    move |ctx: &rozsa_core::config::PostToolUseContext| -> Option<rozsa_core::config::PostToolUseResult> {
                        let tool_name = ctx.tool_name.clone();
                        let is_error = ctx.is_error;
                        // Use try_lock to avoid blocking — if locked, skip recording
                        if let Ok(mut state) = rs.try_lock() {
                            state.record_tool_call(&tool_name, is_error);
                        }
                        None
                    },
                ))
            },
            tools: self.tools.lock().await.clone(),
        })
    }

    /// Persist new messages from agent events into the session file and internal history.
    async fn persist_new_messages(&self, events: &[AgentEvent]) -> Result<()> {
        let mut messages = self.messages.lock().await;
        let mut session_manager = self.session_manager.lock().await;
        for event in events {
            if let AgentEvent::AgentEnd { messages: new } = event {
                for msg in new {
                    messages.push(msg.clone());

                    if let Some(message) = msg.as_standard() {
                        session_manager.append_message(message.clone())?;
                    }
                }
            }
        }
        Ok(())
    }
}

/// Convert AgentMessages to LLM-compatible Messages, filtering out custom messages.
fn convert_to_llm(messages: &[AgentMessage]) -> Vec<Message> {
    messages
        .iter()
        .filter_map(|msg| msg.as_standard().cloned())
        .collect()
}

/// Check if the total token usage has exceeded the compaction threshold.
/// Never stops after a tool-use turn — the model must get one more turn to
/// synthesize an answer from the tool results before compaction fires.
fn should_stop_for_compaction(ctx: &ShouldStopContext, threshold_tokens: u64) -> bool {
    if !ctx.tool_results.is_empty() {
        return false;
    }
    let latest_input = ctx.message.usage.input;
    latest_input >= threshold_tokens
}

/// Build the model_stream function that delegates to rozsa_model's provider registry.
fn build_model_stream_fn() -> ModelStreamFn {
    Box::new(
        |model: &Model, context: &rozsa_model::types::Context, options: &SimpleStreamOptions| {
            rozsa_model::stream::stream_simple(model, context, options)
        },
    )
}

struct ResolvedCredentials {
    api_key: Option<String>,
    headers: Option<std::collections::HashMap<String, String>>,
}

/// Resolve request credentials from environment variables, then OAuth auth.json fallback.
async fn resolve_credentials(model: &Model) -> Result<ResolvedCredentials> {
    use rozsa_model::types::Provider;

    // 1. Try environment variable first
    let env_var = match &model.provider {
        Provider::Anthropic => Some("ANTHROPIC_API_KEY"),
        Provider::OpenAI => Some("OPENAI_API_KEY"),
        Provider::Google | Provider::GoogleVertex => Some("GOOGLE_API_KEY"),
        Provider::DeepSeek => Some("DEEPSEEK_API_KEY"),
        Provider::OpenRouter => Some("OPENROUTER_API_KEY"),
        Provider::XAI => Some("XAI_API_KEY"),
        Provider::Groq => Some("GROQ_API_KEY"),
        Provider::Mistral => Some("MISTRAL_API_KEY"),
        Provider::Together => Some("TOGETHER_API_KEY"),
        Provider::HuggingFace => Some("HF_TOKEN"),
        Provider::Custom(provider) if is_oauth_custom_provider(provider) => None,
        Provider::Custom(_) => Some("LLM_API_KEY"),
        _ => None,
    };
    if let Some(var) = env_var
        && let Ok(key) = std::env::var(var)
        && !key.is_empty()
    {
        return Ok(ResolvedCredentials {
            api_key: Some(key),
            headers: None,
        });
    }

    // 2. Try auth.json only for OAuth providers.
    let Some(provider_name) = oauth_auth_provider_id(&model.provider) else {
        return Ok(ResolvedCredentials {
            api_key: None,
            headers: None,
        });
    };
    let Some(home) = dirs_next::home_dir() else {
        return Ok(ResolvedCredentials {
            api_key: None,
            headers: None,
        });
    };
    let auth_path = home.join(".rozsa").join("models").join("auth.json");
    if auth_path.exists() {
        let path_str = auth_path.to_string_lossy().to_string();
        if let Some(key) =
            rozsa_model::credentials::resolve_auth_json_api_key_pub(&path_str, provider_name)
                .await
                .map_err(|error| anyhow::anyhow!(error))?
        {
            let headers = oauth_request_headers(provider_name, &path_str, &key)?;
            return Ok(ResolvedCredentials {
                api_key: Some(key),
                headers,
            });
        }
    }

    Ok(ResolvedCredentials {
        api_key: None,
        headers: None,
    })
}

fn oauth_auth_provider_id(provider: &rozsa_model::types::Provider) -> Option<&str> {
    match provider {
        rozsa_model::types::Provider::Anthropic => Some("anthropic"),
        rozsa_model::types::Provider::Custom(value) if is_oauth_custom_provider(value) => {
            Some(value.as_str())
        }
        _ => None,
    }
}

fn is_oauth_custom_provider(provider: &str) -> bool {
    provider == "codex-oauth" || provider == "github-copilot"
}

fn oauth_request_headers(
    provider_name: &str,
    auth_path: &str,
    access_token: &str,
) -> Result<Option<std::collections::HashMap<String, String>>> {
    if provider_name != "codex-oauth" {
        return Ok(None);
    }

    let account_id = rozsa_model::credentials::read_account_id(auth_path, provider_name)
        .or_else(|| rozsa_model::oauth::openai_codex::extract_account_id_from_jwt(access_token));
    let Some(account_id) = account_id else {
        anyhow::bail!("codex-oauth credential is missing accountId; run /login again");
    };

    let mut headers = std::collections::HashMap::new();
    headers.insert("x-rozsa-account-id".to_string(), account_id);
    Ok(Some(headers))
}

fn merge_headers(
    base: Option<std::collections::HashMap<String, String>>,
    extra: Option<std::collections::HashMap<String, String>>,
) -> Option<std::collections::HashMap<String, String>> {
    let mut headers = base.unwrap_or_default();
    if let Some(extra_headers) = extra {
        headers.extend(extra_headers);
    }
    (!headers.is_empty()).then_some(headers)
}

/// Current timestamp in milliseconds since UNIX epoch.
fn current_timestamp_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

#[cfg(test)]
mod tests {
    use super::oauth_auth_provider_id;
    use super::oauth_request_headers;
    use rozsa_model::types::Provider;

    #[test]
    fn auth_json_provider_gate_only_allows_oauth_providers() {
        assert_eq!(
            oauth_auth_provider_id(&Provider::Anthropic),
            Some("anthropic")
        );
        assert_eq!(
            oauth_auth_provider_id(&Provider::Custom("codex-oauth".to_string())),
            Some("codex-oauth")
        );
        assert_eq!(
            oauth_auth_provider_id(&Provider::Custom("github-copilot".to_string())),
            Some("github-copilot")
        );
        assert_eq!(oauth_auth_provider_id(&Provider::OpenAI), None);
        assert_eq!(
            oauth_auth_provider_id(&Provider::Custom("qwen3.5".to_string())),
            None
        );
    }

    #[test]
    fn codex_oauth_headers_require_account_id() {
        let err = oauth_request_headers("codex-oauth", "/no/such/auth.json", "not-a-jwt")
            .unwrap_err()
            .to_string();
        assert!(err.contains("missing accountId"));
        assert!(
            oauth_request_headers("anthropic", "/no/such/auth.json", "token")
                .unwrap()
                .is_none()
        );
    }
}
