// FrameworkTree
// agent_session.rs
// ├── struct AgentSessionConfig
// ├── struct ConfigurationReload
// ├── struct StaticConfig
// ├── struct RuntimeParams
// ├── struct AgentSession
// ├── impl AgentSession
// ├── new()
// ├── register_extension()
// ├── skill_registry()
// ├── reload_skills()
// ├── reload_configuration()
// ├── subscribe()
// ├── register_tool()
// ├── registered_tool_metadata()
// ├── register_default_tools()
// ├── register_default_tools_with_question_sender()
// ├── prompt()
// ├── prompt_with_prefix_blocks()
// ├── continue_session()
// ├── drain_and_broadcast()
// ├── abort()
// ├── is_running()
// ├── messages()
// ├── reload_messages_from_session()
// ├── subagent_manager()
// ├── subagent_manager_try_lock()
// ├── viewing_subagent_id()
// ├── viewing_subagent_id_try_lock()
// ├── set_viewing_subagent()
// ├── session_manager()
// ├── switch_session()
// ├── settings_manager()
// ├── cwd()
// ├── current_cwd()
// ├── thinking_effort()
// ├── model()
// ├── set_model()
// ├── set_thinking_effort()
// ├── show_images()
// ├── hide_thinking()
// ├── is_initial_session_name_candidate()
// ├── generate_session_name()
// ├── persist_generated_session_name()
// ├── is_compacting()
// ├── compact()
// ├── compact_inner()
// ├── maybe_auto_compact()
// ├── latest_context_tokens()
// ├── runtime_state_snapshot()
// ├── cycle_edit_mode()
// ├── steer()
// ├── follow_up()
// ├── pending_messages()
// ├── execute_bash()
// ├── expand_skill_command()
// ├── build_agent_context()
// ├── build_loop_config()
// ├── persist_new_messages()
// ├── should_persist_loop_message()
// ├── persisted_user_message_count()
// ├── clean_generated_session_name()
// ├── direct_session_name()
// ├── skill_command_tokens()
// ├── convert_to_llm()
// ├── should_stop_for_compaction()
// ├── usage_context_tokens()
// ├── default_model_stream()
// ├── model_stream_with_thinking_effort_fallback()
// ├── forward_model_stream()
// ├── supported_thinking_efforts()
// ├── normalize_thinking_effort()
// ├── thinking_effort_rank()
// ├── thinking_effort_attempt_values()
// ├── remember_thinking_effort()
// ├── persist_thinking_effort()
// ├── thinking_effort_unavailable_event()
// ├── unsupported_effort_message()
// ├── struct ResolvedCredentials
// ├── resolve_configured_model_api_key()
// ├── resolve_credentials()
// ├── oauth_auth_provider_id()
// ├── is_oauth_custom_provider()
// ├── oauth_request_headers()
// ├── merge_headers()
// ├── current_timestamp_ms()
// ├── mod tests
// ├── auth_json_provider_gate_only_allows_oauth_providers()
// ├── codex_oauth_headers_require_account_id()
// ├── finds_multiple_skill_command_tokens()
// └── prompted_user_message_is_not_persisted_twice_at_agent_end()

// File: agent_session.rs
//
// Internal Framework:
// agent_session.rs
// ├── AgentSessionConfig        # Configuration bundle for session creation
// ├── AgentSession              # Top-level orchestrator
// │   ├── new()                 # Create from config
// │   ├── register_tool()       # Add a single tool
// │   ├── register_default_tools()  # Register read/write/edit/bash/subagent
// │   ├── prompt()              # Send user message and run agent loop
// │   ├── is_initial_session_name_candidate() # Gate the first naming attempt
// │   ├── generate_session_name() # Direct short title or isolated small-model request
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

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::Result;
use tokio::sync::{Mutex, broadcast};
use tokio_util::sync::CancellationToken;

use rozsa_core::agent_loop::{agent_loop, agent_loop_continue};
use rozsa_core::config::{AgentContext, AgentLoopConfig, ShouldStopContext};
use rozsa_core::events::AgentEvent;
use rozsa_core::messages::AgentMessage;
use rozsa_core::tool::{Tool, ToolExecutionMode, ToolMetadata, tool_metadata};
use rozsa_model::event_stream::{EventStream, create_event_stream};
use rozsa_model::types::{
    CacheRetention, ContentBlock, Message, Model, SimpleStreamOptions, StreamOptions,
    ThinkingEffort, ToolSchema, Transport, Usage, UserContent, UserMessage,
};

use crate::model_registry::ModelRegistry;
use crate::resources::LoadedResources;
use crate::session::manager::SessionManager;
use crate::settings::SettingsManager;
use crate::tools::{
    AskUserQuestionRequestSender, create_ask_user_question_tool, create_bash_tool_with_session,
    create_edit_tool, create_read_tool, create_subagent_tool, create_write_tool,
};

/// Configuration bundle for creating an AgentSession.
///
/// Aggregates all dependencies the session orchestrator needs: model selection,
/// system prompt, persistence, settings, and extension hooks.
pub struct AgentSessionConfig {
    /// Model to use for LLM requests.
    pub model: Model,
    /// Thinking/reasoning level for the model.
    pub thinking_effort: ThinkingEffort,
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
    /// Optional model stream override used by an embedded runtime or scripted test.
    pub model_stream: Option<ModelStream>,
}

pub struct ConfigurationReload {
    pub diagnostics: Vec<crate::skills::loader::SkillDiagnostic>,
    pub skill_count: usize,
    pub tool_count: usize,
}

pub type ModelStream = Arc<
    dyn Fn(
            &Model,
            &rozsa_model::types::Context,
            &SimpleStreamOptions,
        ) -> EventStream<rozsa_model::types::StreamEvent>
        + Send
        + Sync,
>;

/// Static (immutable per session) parts of AgentSessionConfig.
struct StaticConfig {
    system_prompt: String,
    cwd: PathBuf,
    settings_manager: std::sync::RwLock<SettingsManager>,
    #[allow(dead_code)]
    resources: LoadedResources,
}

/// Mutable runtime parameters: model and reasoning level can change between turns.
struct RuntimeParams {
    model: Model,
    thinking_effort: ThinkingEffort,
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
    session_manager: Arc<Mutex<SessionManager>>,
    current_cwd: Arc<Mutex<PathBuf>>,
    tools: Arc<Mutex<Vec<Arc<dyn Tool>>>>,
    tool_settings: Arc<std::sync::RwLock<BTreeMap<String, bool>>>,
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
    model_stream: ModelStream,
}

impl AgentSession {
    /// Create a new agent session from configuration.
    pub fn new(config: AgentSessionConfig) -> Self {
        // Capacity 2048: headroom for token-stream bursts and tool update events.
        let (event_tx, _) = broadcast::channel(2048);
        let AgentSessionConfig {
            model,
            thinking_effort,
            system_prompt,
            cwd,
            session_manager,
            settings_manager,
            resources,
            pre_tool_use,
            model_stream,
        } = config;
        let thinking_effort = normalize_thinking_effort(&model, thinking_effort);
        let restored_messages = session_manager
            .context_messages()
            .into_iter()
            .map(AgentMessage::standard)
            .collect::<Vec<_>>();
        let session_manager_id = session_manager.session_id().to_string();
        let session_manager_file = Some(session_manager.session_file().to_path_buf());
        let session_manager = Arc::new(Mutex::new(session_manager));
        let current_cwd = Arc::new(Mutex::new(cwd.clone()));
        let permission_mode = settings_manager.resolved().permissions.mode.clone();
        let config_roots = crate::config_paths::ConfigRoots::discover(&cwd)
            .expect("Rózsa config roots must be available before loading skills");
        let skill_registry = crate::skills::SkillRegistry::load_from_roots_with_settings(
            &config_roots,
            &settings_manager.resolved().skills,
        );
        let tool_settings = Arc::new(std::sync::RwLock::new(
            settings_manager.resolved().tools.clone(),
        ));

        let tools_arc: Arc<Mutex<Vec<Arc<dyn Tool>>>> = Arc::new(Mutex::new(Vec::new()));
        let main_session_uuid = session_manager_id;
        let main_session_file = session_manager_file;
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

        let global_models_dir = config_roots.global().join("models");
        let model_stream = model_stream.unwrap_or_else(|| default_model_stream(global_models_dir));
        let shared = crate::subagent::SharedResources {
            model_stream: model_stream.clone(),
            convert_to_llm: Arc::new(convert_to_llm),
            main_tools: tools_arc.clone(),
            tool_settings: tool_settings.clone(),
            main_model: model.clone(),
            main_thinking_effort: thinking_effort,
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
                settings_manager: std::sync::RwLock::new(settings_manager),
                resources,
            },
            runtime: Mutex::new(RuntimeParams {
                model,
                thinking_effort,
            }),
            session_manager,
            current_cwd,
            tools: tools_arc,
            tool_settings,
            cancel_token: Mutex::new(None),
            is_running: AtomicBool::new(false),
            messages: Mutex::new(restored_messages),
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
            model_stream,
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
        let settings = self
            .static_config
            .settings_manager
            .read()
            .unwrap()
            .resolved()
            .skills
            .clone();
        let roots = crate::config_paths::ConfigRoots::discover(&self.static_config.cwd)
            .expect("Rózsa config roots must be available before loading skills");
        let (new_registry, diagnostics) =
            crate::skills::SkillRegistry::load_from_roots_with_settings_and_diagnostics(
                &roots, &settings,
            );
        *self.skill_registry.write().unwrap() = new_registry;
        diagnostics
    }

    pub async fn reload_configuration(&self) -> Result<ConfigurationReload> {
        let settings = {
            let mut settings_manager = self.static_config.settings_manager.write().unwrap();
            settings_manager.reload()?;
            settings_manager.resolved().clone()
        };
        *self.tool_settings.write().unwrap() = settings.tools.clone();
        let roots = crate::config_paths::ConfigRoots::discover(&self.static_config.cwd)?;
        let (new_registry, diagnostics) =
            crate::skills::SkillRegistry::load_from_roots_with_settings_and_diagnostics(
                &roots,
                &settings.skills,
            );
        *self.skill_registry.write().unwrap() = new_registry;
        let skill_count = self.skill_registry.read().unwrap().list().len();
        let tool_count = self
            .tools
            .lock()
            .await
            .iter()
            .filter(|tool| settings.tools.get(tool.name()).copied().unwrap_or(true))
            .count();
        Ok(ConfigurationReload {
            diagnostics,
            skill_count,
            tool_count,
        })
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

    pub async fn registered_tool_metadata(&self) -> Vec<ToolMetadata> {
        let tools = self.tools.lock().await;
        tool_metadata(tools.iter().map(|tool| tool.as_ref()))
    }

    /// Register the default built-in tools (read, write, edit, bash, subagent).
    pub async fn register_default_tools(&self, cwd: &Path) {
        self.register_default_tools_with_question_sender(cwd, None)
            .await;
    }

    /// Register built-in tools and, when an interactive frontend is available,
    /// the session-scoped askUserQuestion tool.
    pub async fn register_default_tools_with_question_sender(
        &self,
        cwd: &Path,
        question: Option<(String, AskUserQuestionRequestSender)>,
    ) {
        let defaults: Vec<Box<dyn Tool>> = vec![
            create_read_tool(),
            create_write_tool(),
            create_edit_tool(),
            create_bash_tool_with_session(
                cwd.to_path_buf(),
                self.current_cwd.clone(),
                self.session_manager.clone(),
            ),
            create_subagent_tool(self.subagent_manager.clone()),
        ];
        let mut defaults = defaults;
        if let Some((session_id, request_tx)) = question {
            defaults.push(create_ask_user_question_tool(session_id, request_tx));
        }
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
        if let Err(error) = self.persist_new_messages(&events).await {
            *self.cancel_token.lock().await = None;
            self.is_running.store(false, Ordering::SeqCst);
            return Err(error);
        }
        if let Err(error) = self.maybe_auto_compact().await {
            *self.cancel_token.lock().await = None;
            self.is_running.store(false, Ordering::SeqCst);
            return Err(error);
        }

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
        if let Err(error) = self.persist_new_messages(&events).await {
            *self.cancel_token.lock().await = None;
            self.is_running.store(false, Ordering::SeqCst);
            return Err(error);
        }
        if let Err(error) = self.maybe_auto_compact().await {
            *self.cancel_token.lock().await = None;
            self.is_running.store(false, Ordering::SeqCst);
            return Err(error);
        }

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
    /// gracefully at the next check point. Pending steering and follow-up input
    /// belongs to the stopped interaction and is discarded. Safe to call
    /// concurrently with `prompt`.
    pub async fn abort(&self) {
        self.steering_queue.lock().unwrap().clear();
        self.follow_up_queue.lock().unwrap().clear();
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

    /// Rebuild the in-memory conversation from the current session branch.
    ///
    /// GUI fork setup appends the selected history after constructing the
    /// AgentSession, so it must explicitly synchronize the agent before the
    /// first prompt.
    pub async fn reload_messages_from_session(&self) {
        let restored_messages = self
            .session_manager
            .lock()
            .await
            .context_messages()
            .into_iter()
            .map(AgentMessage::standard)
            .collect();
        *self.messages.lock().await = restored_messages;
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
    /// and loads that branch's persisted conversation history. Returns the old
    /// session path.
    pub async fn switch_session(
        &self,
        path: impl AsRef<std::path::Path>,
    ) -> anyhow::Result<String> {
        let new_mgr = SessionManager::open(&path)?;
        let restored_messages = new_mgr
            .context_messages()
            .into_iter()
            .map(AgentMessage::standard)
            .collect();
        let mut mgr = self.session_manager.lock().await;
        let new_cwd = PathBuf::from(new_mgr.cwd());
        let old_path = mgr.session_file().to_string_lossy().to_string();
        *mgr = new_mgr;
        drop(mgr);
        *self.current_cwd.lock().await = new_cwd;
        *self.messages.lock().await = restored_messages;
        Ok(old_path)
    }

    /// Get the settings manager (read-only, immutable for the session lifetime).
    pub fn settings_manager(&self) -> SettingsManager {
        self.static_config.settings_manager.read().unwrap().clone()
    }

    /// Get the working directory.
    pub fn cwd(&self) -> &Path {
        &self.static_config.cwd
    }

    /// Get the session's persisted and currently active working directory.
    pub async fn current_cwd(&self) -> PathBuf {
        self.current_cwd.lock().await.clone()
    }

    /// Get the current thinking effort.
    pub async fn thinking_effort(&self) -> ThinkingEffort {
        self.runtime.lock().await.thinking_effort
    }

    /// Get the current model (cloned snapshot).
    pub async fn model(&self) -> Model {
        self.runtime.lock().await.model.clone()
    }

    /// Update the model for subsequent turns.
    pub async fn set_model(&self, model: Model) {
        let mut runtime = self.runtime.lock().await;
        runtime.thinking_effort = normalize_thinking_effort(&model, runtime.thinking_effort);
        runtime.model = model;
    }

    /// Update the thinking effort for subsequent turns.
    pub async fn set_thinking_effort(&self, effort: ThinkingEffort) {
        self.runtime.lock().await.thinking_effort = effort;
    }

    // --- Phase A: State accessors ---

    /// Get show_images setting.
    pub fn show_images(&self) -> bool {
        !self
            .static_config
            .settings_manager
            .read()
            .unwrap()
            .resolved()
            .block_images
    }

    /// Get hide_thinking flag (true when thinking is off).
    pub async fn hide_thinking(&self) -> bool {
        self.runtime.lock().await.thinking_effort == ThinkingEffort::Off
    }

    /// Return whether the next real user turn is eligible to name this session.
    pub async fn is_initial_session_name_candidate(&self) -> bool {
        let manager = self.session_manager.lock().await;
        manager.current_name().is_none() && persisted_user_message_count(&manager) == 0
    }

    /// Generate and persist a concise name for a session's first real user turn.
    ///
    /// Short input is used directly. Longer input requires an explicitly
    /// selected small model, uses fixed Low reasoning, and is isolated from
    /// conversation history and tools. A second name check before persistence
    /// ensures manual rename always wins.
    pub async fn generate_session_name(
        &self,
        first_user_message: &str,
        small_model: Option<rozsa_model::types::Model>,
    ) -> Result<Option<String>> {
        let first_user_message = first_user_message.trim();
        if first_user_message.is_empty() {
            return Ok(None);
        }
        {
            let manager = self.session_manager.lock().await;
            if manager.current_name().is_some() {
                return Ok(None);
            }
        }

        if let Some(title) = direct_session_name(first_user_message) {
            return self.persist_generated_session_name(title).await;
        }

        let Some(model) = small_model else {
            return Ok(None);
        };
        let credentials = resolve_credentials(&model, &self.static_config.cwd).await?;
        let context = rozsa_model::types::Context {
            system_prompt: Some(
                "Create a concise session title for the user's coding task. \
                 Use the same language as the user. Return only the title. \
                 Use at most 8 words, or at most 24 characters for languages without spaces. \
                 Do not explain, reason, add labels, quotes, or ending punctuation."
                    .to_string(),
            ),
            messages: vec![Message::User(UserMessage {
                content: UserContent::Text(first_user_message.to_string()),
                display_text: None,
                timestamp: current_timestamp_ms(),
            })],
            tools: Vec::new(),
        };
        let retry = self
            .static_config
            .settings_manager
            .read()
            .unwrap()
            .resolved()
            .retry
            .clone();
        let options = SimpleStreamOptions {
            base: StreamOptions {
                temperature: None,
                max_tokens: Some(32),
                api_key: credentials.api_key,
                transport: Transport::Auto,
                cache_retention: CacheRetention::Short,
                session_id: None,
                headers: merge_headers(model.headers.clone(), credentials.headers),
                timeout_ms: retry.timeout_ms,
                max_retries: Some(2),
                max_retry_delay_ms: retry.max_retry_delay_ms,
                metadata: None,
            },
            reasoning: Some(ThinkingEffort::Low),
            thinking_effort_budgets: None,
            tool_choice: None,
        };
        let mut stream = (self.model_stream)(&model, &context, &options);
        let mut raw_title = String::new();
        while let Some(event) = stream.next().await {
            match event {
                rozsa_model::types::StreamEvent::Done { message, .. } => {
                    for block in message.content {
                        if let ContentBlock::Text { text, .. } = block {
                            raw_title.push_str(&text);
                        }
                    }
                    break;
                }
                rozsa_model::types::StreamEvent::Error { error, .. } => {
                    anyhow::bail!(
                        "Session naming request failed: {}",
                        error
                            .error_message
                            .unwrap_or_else(|| "provider returned an error".to_string())
                    );
                }
                _ => {}
            }
        }
        let title = clean_generated_session_name(&raw_title)
            .ok_or_else(|| anyhow::anyhow!("Session naming request returned no usable title"))?;

        self.persist_generated_session_name(title).await
    }

    async fn persist_generated_session_name(&self, title: String) -> Result<Option<String>> {
        let mut manager = self.session_manager.lock().await;
        if manager.current_name().is_some() {
            return Ok(None);
        }
        manager.append_session_info(Some(title.clone()))?;
        Ok(Some(title))
    }

    // --- Phase B: Compaction ---

    /// Whether compaction is currently running.
    pub fn is_compacting(&self) -> bool {
        self.is_compacting.load(Ordering::SeqCst)
    }

    /// Run compaction: abort loop, summarize old messages, replace history.
    pub async fn compact(&self) -> Result<crate::compaction::CompactionResult> {
        self.is_compacting.store(true, Ordering::SeqCst);
        let result = self.compact_inner().await;
        self.is_compacting.store(false, Ordering::SeqCst);
        result
    }

    async fn compact_inner(&self) -> Result<crate::compaction::CompactionResult> {
        use crate::compaction::{CompactionEngine, CompactionTrigger};

        // Abort running loop if any
        if self.is_running() {
            self.abort().await;
            // Give the loop time to stop
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }

        let settings = self
            .static_config
            .settings_manager
            .read()
            .unwrap()
            .resolved()
            .clone();
        let runtime = self.runtime.lock().await;
        let model = runtime.model.clone();
        let thinking_effort = runtime.thinking_effort;
        drop(runtime);
        let token_limits = settings
            .compaction
            .resolve_token_limits(model.context_window)
            .map_err(anyhow::Error::msg)?;
        let engine = CompactionEngine::new(CompactionTrigger {
            threshold_tokens: token_limits.threshold_tokens,
            target_tokens: token_limits.target_tokens,
        });

        let entries = self.session_manager.lock().await.entries();
        let context_tokens = self.latest_context_tokens().await;
        let plan = engine.prepare_with_context(&entries, context_tokens);

        let Some(plan) = plan else {
            anyhow::bail!("Nothing to compact — token usage below threshold");
        };

        let credentials = resolve_credentials(&model, &self.static_config.cwd).await?;
        let model_stream = self.model_stream.clone();
        let summarize_fn = |content: String| {
            let model = model.clone();
            let model_stream = model_stream.clone();
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
                let reasoning = match thinking_effort {
                    ThinkingEffort::Off => None,
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
                    thinking_effort_budgets: None,
                    tool_choice: None,
                };
                let mut stream = model_stream(&model, &context, &options);
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

        // Rebuild from the exact persisted entry boundary. The old code used
        // `messages.len() - removed_count`, but removed_count counted metadata
        // entries as well as messages. That could leave a ToolResult without
        // its preceding Assistant tool call in the next provider prompt.
        let kept_messages =
            entries[plan.cut_point_index..]
                .iter()
                .filter_map(|entry| match entry {
                    crate::session::manager::SessionEntry::Message(message) => {
                        Some(AgentMessage::standard(message.message.clone()))
                    }
                    _ => None,
                });
        let mut messages = self.messages.lock().await;
        let summary_msg = AgentMessage::custom(
            "compaction_summary".to_string(),
            serde_json::json!({ "summary": &result.summary }),
            current_timestamp_ms(),
        );
        *messages = std::iter::once(summary_msg).chain(kept_messages).collect();
        drop(messages);

        Ok(result)
    }

    async fn maybe_auto_compact(&self) -> Result<()> {
        let settings = self
            .static_config
            .settings_manager
            .read()
            .unwrap()
            .resolved()
            .clone();
        if !settings.compaction.enabled {
            return Ok(());
        }
        let context_window = self.runtime.lock().await.model.context_window;
        let token_limits = settings
            .compaction
            .resolve_token_limits(context_window)
            .map_err(anyhow::Error::msg)?;
        let context_tokens = self.latest_context_tokens().await;
        if context_tokens < token_limits.threshold_tokens {
            return Ok(());
        }
        self.compact().await.map(|_| ())
    }

    async fn latest_context_tokens(&self) -> u64 {
        self.messages
            .lock()
            .await
            .iter()
            .rev()
            .find_map(|message| match message.as_standard()? {
                Message::Assistant(assistant) => Some(usage_context_tokens(&assistant.usage)),
                _ => None,
            })
            .unwrap_or(0)
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

    /// Expand all `/skill:name` tokens into XML blocks, returning (expanded_text, display_text).
    /// If no skill command is found, returns (original, None).
    fn expand_skill_command(&self, text: &str) -> (String, Option<String>) {
        let registry = self.skill_registry.read().unwrap();

        let mut expanded = String::new();
        let mut cursor = 0;
        let mut changed = false;
        for (start, end, skill_name) in skill_command_tokens(text) {
            let Some(skill) = registry.find_by_name(skill_name) else {
                continue;
            };
            let content = match std::fs::read_to_string(&skill.file_path) {
                Ok(c) => c,
                Err(_) => continue,
            };
            let body = crate::skills::loader::strip_frontmatter(&content).trim();
            let base_dir = skill.base_dir.display();
            expanded.push_str(&text[cursor..start]);
            expanded.push_str(&format!(
                "<skill>\n<name>{}</name>\n<content>\n{}\n</content>\n<base_dir>{}</base_dir>\n</skill>",
                skill_name, body, base_dir
            ));
            cursor = end;
            changed = true;
        }

        if !changed {
            return (text.to_string(), None);
        }

        expanded.push_str(&text[cursor..]);
        (expanded, Some(text.to_string()))
    }

    /// Build an AgentContext from the current session state.
    async fn build_agent_context(&self) -> AgentContext {
        let tool_settings = self.tool_settings.read().unwrap().clone();
        let tool_schemas: Vec<ToolSchema> = self
            .tools
            .lock()
            .await
            .iter()
            .filter(|tool| tool_settings.get(tool.name()).copied().unwrap_or(true))
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
        let thinking_effort = runtime.thinking_effort;
        drop(runtime);
        let settings = self
            .static_config
            .settings_manager
            .read()
            .unwrap()
            .resolved()
            .clone();

        // Build stream options from settings
        let reasoning = match thinking_effort {
            ThinkingEffort::Off => None,
            level => Some(level),
        };

        let credentials = resolve_credentials(&model, &self.static_config.cwd).await?;

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
            thinking_effort_budgets: None,
            tool_choice: None,
        };

        // Resolve the configured ratio against the active model's context
        // window only at execution time.
        let compaction_threshold = settings
            .compaction
            .resolve_token_limits(model.context_window)
            .map_err(anyhow::Error::msg)?
            .threshold_tokens;
        let compaction_enabled = settings.compaction.enabled;

        // Clone queues and state for closure capture
        let steering_q = self.steering_queue.clone();
        let follow_up_q = self.follow_up_queue.clone();
        let runtime_state_for_post = self.runtime_state.clone();

        Ok(AgentLoopConfig {
            model: model.clone(),
            reasoning,
            stream_options,
            model_stream: {
                let model_stream = self.model_stream.clone();
                Box::new(move |model, context, options| model_stream(model, context, options))
            },
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
                        // prompt_with_prefix_blocks persists the user message
                        // before the loop starts so an interrupted prompt is
                        // still recoverable. AgentEnd contains that same user
                        // message, so only persist the loop-produced messages
                        // here.
                        if should_persist_loop_message(message) {
                            session_manager.append_message(message.clone())?;
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

fn should_persist_loop_message(message: &Message) -> bool {
    !matches!(message, Message::User(_))
}

fn persisted_user_message_count(manager: &SessionManager) -> usize {
    manager
        .entries()
        .iter()
        .filter(|entry| {
            matches!(
                entry,
                crate::session::manager::SessionEntry::Message(message)
                    if matches!(message.message, Message::User(_))
            )
        })
        .count()
}

fn clean_generated_session_name(raw: &str) -> Option<String> {
    let mut cleaned = raw.to_string();
    while let Some(start) = cleaned.find("<think>") {
        let Some(relative_end) = cleaned[start + "<think>".len()..].find("</think>") else {
            cleaned.truncate(start);
            break;
        };
        let end = start + "<think>".len() + relative_end + "</think>".len();
        cleaned.replace_range(start..end, "");
    }
    let line = cleaned
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())?;
    let line = line
        .strip_prefix("Title:")
        .or_else(|| line.strip_prefix("title:"))
        .unwrap_or(line)
        .trim()
        .trim_matches(['"', '\'', '`']);
    let normalized = line.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.is_empty() {
        return None;
    }
    if normalized.chars().count() <= 60 {
        return Some(normalized);
    }
    Some(format!(
        "{}...",
        normalized.chars().take(57).collect::<String>()
    ))
}

fn direct_session_name(input: &str) -> Option<String> {
    let normalized = input.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.is_empty() || normalized.split_whitespace().count() >= 8 {
        return None;
    }
    let contains_cjk = normalized.chars().any(|character| {
        matches!(
            character as u32,
            0x2E80..=0x9FFF | 0xF900..=0xFAFF | 0x20000..=0x2FA1F
        )
    });
    let max_characters = if contains_cjk { 24 } else { 60 };
    (normalized.chars().count() <= max_characters).then_some(normalized)
}

fn skill_command_tokens(text: &str) -> Vec<(usize, usize, &str)> {
    let mut tokens = Vec::new();
    for (start, ch) in text.char_indices() {
        if ch != '/' || (start > 0 && !text[..start].ends_with(char::is_whitespace)) {
            continue;
        }
        let command_start = start + ch.len_utf8();
        if !text[command_start..].starts_with("skill:") {
            continue;
        }
        let command_end = text[command_start..]
            .char_indices()
            .find_map(|(idx, ch)| ch.is_whitespace().then_some(command_start + idx))
            .unwrap_or(text.len());
        let skill_name_start = command_start + "skill:".len();
        if command_end == skill_name_start {
            continue;
        }
        tokens.push((start, command_end, &text[skill_name_start..command_end]));
    }
    tokens
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
    usage_context_tokens(&ctx.message.usage) >= threshold_tokens
}

fn usage_context_tokens(usage: &Usage) -> u64 {
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

/// Build the default model stream that delegates to rozsa_model's provider registry.
fn default_model_stream(global_models_dir: PathBuf) -> ModelStream {
    model_stream_with_thinking_effort_fallback(
        global_models_dir,
        Arc::new(|model, context, options| {
            rozsa_model::stream::stream_simple(model, context, options)
        }),
    )
}

/// Wrap a provider stream with learned, safe thinking effort fallbacks.
pub fn model_stream_with_thinking_effort_fallback(
    global_models_dir: PathBuf,
    attempt_stream: ModelStream,
) -> ModelStream {
    let learned_efforts = Arc::new(std::sync::Mutex::new(HashMap::new()));
    Arc::new(
        move |model: &Model,
              context: &rozsa_model::types::Context,
              options: &SimpleStreamOptions| {
            let (sender, stream) = create_event_stream();
            let model = model.clone();
            let context = context.clone();
            let options = options.clone();
            let global_models_dir = global_models_dir.clone();
            let learned_efforts = learned_efforts.clone();
            let attempt_stream = attempt_stream.clone();
            tokio::spawn(async move {
                let Some(effort) = options.reasoning else {
                    forward_model_stream(&sender, &attempt_stream, &model, &context, &options)
                        .await;
                    return;
                };
                if effort == ThinkingEffort::Off {
                    forward_model_stream(&sender, &attempt_stream, &model, &context, &options)
                        .await;
                    return;
                }

                let model_key = format!("{}/{}", model.provider.as_str(), model.id);
                let mut effective_model = model.clone();
                if let Some(learned) = learned_efforts
                    .lock()
                    .expect("learned thinking effort lock must not be poisoned")
                    .get(&model_key)
                    .cloned()
                {
                    let mut map = effective_model
                        .thinking_effort_map
                        .take()
                        .unwrap_or_default();
                    map.extend(learned);
                    effective_model.thinking_effort_map = Some(map);
                }
                let candidates = thinking_effort_attempt_values(&effective_model, effort);
                if candidates.is_empty() {
                    sender.push(thinking_effort_unavailable_event(&model, effort));
                    return;
                }

                for (attempt, value) in candidates.iter().enumerate() {
                    let mut attempt_model = effective_model.clone();
                    let mut map = attempt_model.thinking_effort_map.take().unwrap_or_default();
                    map.insert(effort, Some(value.clone()));
                    attempt_model.thinking_effort_map = Some(map);

                    let mut inner = attempt_stream(&attempt_model, &context, &options);
                    let mut retry = false;
                    let mut emitted_response = false;
                    let mut succeeded = false;
                    while let Some(event) = inner.next().await {
                        if let rozsa_model::types::StreamEvent::Error { error, .. } = &event {
                            let message = error.error_message.as_deref().unwrap_or_default();
                            if !emitted_response && unsupported_effort_message(message) {
                                if attempt + 1 < candidates.len() {
                                    retry = true;
                                    break;
                                }
                                remember_thinking_effort(
                                    &learned_efforts,
                                    &model_key,
                                    effort,
                                    None,
                                );
                                if let Err(error) = persist_thinking_effort(
                                    &global_models_dir,
                                    &model,
                                    effort,
                                    None,
                                ) {
                                    tracing::warn!(%error, provider = %model.provider, model = %model.id, "failed to persist unsupported thinking effort");
                                }
                            }
                        } else {
                            emitted_response = true;
                            succeeded |=
                                matches!(event, rozsa_model::types::StreamEvent::Done { .. });
                        }
                        sender.push(event);
                    }
                    if retry {
                        continue;
                    }
                    if succeeded {
                        remember_thinking_effort(
                            &learned_efforts,
                            &model_key,
                            effort,
                            Some(value.clone()),
                        );
                        if let Err(error) =
                            persist_thinking_effort(&global_models_dir, &model, effort, Some(value))
                        {
                            tracing::warn!(%error, provider = %model.provider, model = %model.id, "failed to persist learned thinking effort");
                        }
                    }
                    break;
                }
            });
            stream
        },
    )
}

async fn forward_model_stream(
    sender: &rozsa_model::event_stream::EventStreamSender<rozsa_model::types::StreamEvent>,
    attempt_stream: &ModelStream,
    model: &Model,
    context: &rozsa_model::types::Context,
    options: &SimpleStreamOptions,
) {
    let mut inner = attempt_stream(model, context, options);
    while let Some(event) = inner.next().await {
        sender.push(event);
    }
}

const THINKING_EFFORT_ORDER: [ThinkingEffort; 6] = [
    ThinkingEffort::Off,
    ThinkingEffort::Low,
    ThinkingEffort::Medium,
    ThinkingEffort::High,
    ThinkingEffort::XHigh,
    ThinkingEffort::Max,
];

/// Return the logical thinking efforts that the model explicitly supports.
pub fn supported_thinking_efforts(model: &Model) -> Vec<ThinkingEffort> {
    if !model.reasoning {
        return vec![ThinkingEffort::Off];
    }

    THINKING_EFFORT_ORDER
        .into_iter()
        .filter(|effort| {
            *effort == ThinkingEffort::Off
                || !matches!(
                    model
                        .thinking_effort_map
                        .as_ref()
                        .and_then(|map| map.get(effort)),
                    Some(None)
                )
        })
        .collect()
}

/// Clamp an effort to the nearest supported lower effort for a model.
pub fn normalize_thinking_effort(model: &Model, requested: ThinkingEffort) -> ThinkingEffort {
    let requested_rank = thinking_effort_rank(requested);
    supported_thinking_efforts(model)
        .into_iter()
        .filter(|effort| thinking_effort_rank(*effort) <= requested_rank)
        .max_by_key(|effort| thinking_effort_rank(*effort))
        .unwrap_or(ThinkingEffort::Off)
}

fn thinking_effort_rank(effort: ThinkingEffort) -> usize {
    match effort {
        ThinkingEffort::Off => 0,
        ThinkingEffort::Low => 1,
        ThinkingEffort::Medium => 2,
        ThinkingEffort::High => 3,
        ThinkingEffort::XHigh => 4,
        ThinkingEffort::Max => 5,
    }
}

/// Return provider-facing values to try for one logical thinking effort.
pub fn thinking_effort_attempt_values(model: &Model, effort: ThinkingEffort) -> Vec<String> {
    let mut candidates = match effort {
        ThinkingEffort::Low => vec!["low", "light", "minimal"],
        ThinkingEffort::Medium => vec!["medium"],
        ThinkingEffort::High => vec!["high"],
        ThinkingEffort::XHigh => vec!["xhigh"],
        ThinkingEffort::Max => vec!["max"],
        ThinkingEffort::Off => Vec::new(),
    }
    .into_iter()
    .map(str::to_string)
    .collect::<Vec<_>>();
    match model
        .thinking_effort_map
        .as_ref()
        .and_then(|map| map.get(&effort))
    {
        Some(Some(mapped)) => {
            if effort != ThinkingEffort::Low {
                return vec![mapped.clone()];
            }
            candidates.retain(|candidate| candidate != mapped);
            candidates.insert(0, mapped.clone());
            candidates
        }
        Some(None) => Vec::new(),
        None => candidates,
    }
}

fn remember_thinking_effort(
    learned_efforts: &std::sync::Mutex<HashMap<String, HashMap<ThinkingEffort, Option<String>>>>,
    model_key: &str,
    effort: ThinkingEffort,
    value: Option<String>,
) {
    learned_efforts
        .lock()
        .expect("learned thinking effort lock must not be poisoned")
        .entry(model_key.to_string())
        .or_default()
        .insert(effort, value);
}

fn persist_thinking_effort(
    global_models_dir: &Path,
    model: &Model,
    effort: ThinkingEffort,
    value: Option<&str>,
) -> Result<()> {
    ModelRegistry::load_from_dir(global_models_dir)?.persist_thinking_effort(
        model.provider.as_str(),
        &model.id,
        effort,
        value,
    )?;
    Ok(())
}

fn thinking_effort_unavailable_event(
    model: &Model,
    effort: ThinkingEffort,
) -> rozsa_model::types::StreamEvent {
    let mut error = rozsa_model::providers::common::create_output(model, model.api.clone());
    error.stop_reason = rozsa_model::types::StopReason::Error;
    error.error_message = Some(format!(
        "Thinking effort {effort:?} is disabled for {}/{} after the provider explicitly rejected it.",
        model.provider, model.id
    ));
    rozsa_model::types::StreamEvent::Error {
        reason: rozsa_model::types::StopReason::Error,
        error,
    }
}

fn unsupported_effort_message(message: &str) -> bool {
    rozsa_model::providers::common::is_explicit_unsupported_thinking_effort_error(message)
}

struct ResolvedCredentials {
    api_key: Option<String>,
    headers: Option<std::collections::HashMap<String, String>>,
}

/// Resolve a configured provider API key without putting it into model metadata.
pub(crate) fn resolve_configured_model_api_key(
    model: &Model,
    cwd: &Path,
) -> Result<Option<String>> {
    let roots =
        crate::config_paths::ConfigRoots::discover(cwd).map_err(|error| anyhow::anyhow!(error))?;
    let [global_models_dir, project_models_dir] = roots.model_dirs();
    let (registry, _) =
        ModelRegistry::load_from_dirs_with_diagnostics(&[&global_models_dir, &project_models_dir])
            .map_err(|error| anyhow::anyhow!(error))?;
    let Some(reference) = registry.provider_api_key_reference(model.provider.as_str()) else {
        return Ok(None);
    };
    rozsa_model::credentials::resolve_config_value(reference)
        .map(Some)
        .map_err(|error| anyhow::anyhow!(error))
}

/// Resolve request credentials from model config, environment variables, then
/// OAuth auth.json fallback.
async fn resolve_credentials(model: &Model, cwd: &Path) -> Result<ResolvedCredentials> {
    if let Some(api_key) = resolve_configured_model_api_key(model, cwd)? {
        return Ok(ResolvedCredentials {
            api_key: Some(api_key),
            headers: None,
        });
    }

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
    if let Some(var) = env_var {
        if let Some(key) = rozsa_model::credentials::resolve_environment_variable(var)
            .map_err(|error| anyhow::anyhow!(error))?
        {
            return Ok(ResolvedCredentials {
                api_key: Some(key),
                headers: None,
            });
        }
    }

    // 2. Try auth.json only for OAuth providers.
    let Some(provider_name) = oauth_auth_provider_id(&model.provider) else {
        return Ok(ResolvedCredentials {
            api_key: None,
            headers: None,
        });
    };
    let auth_path = crate::config_paths::ConfigRoots::global_models_dir()
        .map_err(|error| anyhow::anyhow!(error))?
        .join("auth.json");
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
    use super::skill_command_tokens;
    use rozsa_model::types::{Message, Provider, UserContent, UserMessage};

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

    #[test]
    fn finds_multiple_skill_command_tokens() {
        let tokens = skill_command_tokens("prefix /skill:ask and /skill:brainstorm suffix");
        assert_eq!(tokens, vec![(7, 17, "ask"), (22, 39, "brainstorm")]);
    }

    #[test]
    fn prompted_user_message_is_not_persisted_twice_at_agent_end() {
        let message = Message::User(UserMessage {
            content: UserContent::Text("hello".to_string()),
            display_text: None,
            timestamp: 0,
        });
        assert!(!super::should_persist_loop_message(&message));
    }
}
