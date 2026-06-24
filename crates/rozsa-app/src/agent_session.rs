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
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::Result;
use tokio::sync::{broadcast, Mutex};
use tokio_util::sync::CancellationToken;

use rozsa_core::agent_loop::{agent_loop, agent_loop_continue};
use rozsa_core::config::{AgentContext, AgentLoopConfig, ModelStreamFn, ShouldStopContext};
use rozsa_core::events::AgentEvent;
use rozsa_core::messages::AgentMessage;
use rozsa_core::tool::{Tool, ToolExecutionMode};
use rozsa_model::event_stream::EventStream;
use rozsa_model::types::{
    CacheRetention, Message, Model, SimpleStreamOptions, StreamOptions, ThinkingLevel, ToolSchema,
    Transport, UserContent, UserMessage,
};

use crate::resources::LoadedResources;
use crate::session::manager::SessionManager;
use crate::settings::SettingsManager;
use crate::tools::{
    create_bash_tool, create_edit_tool, create_find_tool, create_grep_tool, create_ls_tool,
    create_read_tool, create_write_tool,
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
    tools: Mutex<Vec<Arc<dyn Tool>>>,
    cancel_token: Mutex<Option<CancellationToken>>,
    is_running: AtomicBool,
    /// Accumulated messages across turns (the conversation history).
    messages: Mutex<Vec<AgentMessage>>,
    /// Broadcast channel for AgentEvents — subscribers see every event the loop emits.
    event_tx: broadcast::Sender<AgentEvent>,
}

impl AgentSession {
    /// Create a new agent session from configuration.
    pub fn new(config: AgentSessionConfig) -> Self {
        // Capacity 256: enough headroom for token-stream bursts; slow subscribers will lag.
        let (event_tx, _) = broadcast::channel(256);
        let AgentSessionConfig {
            model,
            thinking_level,
            system_prompt,
            cwd,
            session_manager,
            settings_manager,
            resources,
        } = config;
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
            tools: Mutex::new(Vec::new()),
            cancel_token: Mutex::new(None),
            is_running: AtomicBool::new(false),
            messages: Mutex::new(Vec::new()),
            event_tx,
        }
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

    /// Register the default built-in tools (read, write, edit, bash, ls, grep, find).
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

        // Build user message
        let user_msg = AgentMessage::standard(Message::User(UserMessage {
            content: UserContent::Text(message.to_string()),
            display_text: None,
            timestamp: current_timestamp_ms(),
        }));

        // Persist user message to session file
        self.session_manager
            .lock()
            .await
            .append_message(Message::User(UserMessage {
                content: UserContent::Text(message.to_string()),
                display_text: None,
                timestamp: current_timestamp_ms(),
            }))?;

        let context = self.build_agent_context().await;
        let loop_config = self.build_loop_config().await;

        let stream = agent_loop(
            vec![user_msg],
            context,
            loop_config,
            Some(cancel_token),
        );

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
        let loop_config = self.build_loop_config().await;

        let stream = agent_loop_continue(context, loop_config, Some(cancel_token));

        let events = self.drain_and_broadcast(stream).await;
        self.persist_new_messages(&events).await?;

        *self.cancel_token.lock().await = None;
        self.is_running.store(false, Ordering::SeqCst);

        Ok(events)
    }

    /// Drain an EventStream while fan-out broadcasting each event to subscribers.
    /// Returns the same Vec the old `collect_events` produced — callers see no behavior change.
    async fn drain_and_broadcast(
        &self,
        mut stream: EventStream<AgentEvent>,
    ) -> Vec<AgentEvent> {
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

    /// Lock and access the session manager. Holds the lock for the duration of the borrow —
    /// keep usage short to avoid blocking concurrent operations.
    pub async fn session_manager(&self) -> tokio::sync::MutexGuard<'_, SessionManager> {
        self.session_manager.lock().await
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

    // --- Private helpers ---

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

        AgentContext {
            system_prompt: Some(self.static_config.system_prompt.clone()),
            messages: self.messages.lock().await.clone(),
            tools: tool_schemas,
        }
    }

    /// Build the AgentLoopConfig wiring all hooks to this session's dependencies.
    async fn build_loop_config(&self) -> AgentLoopConfig {
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

        let api_key = resolve_api_key(&model);

        let stream_options = SimpleStreamOptions {
            base: StreamOptions {
                temperature: None,
                max_tokens: Some(model.max_tokens),
                api_key,
                transport: Transport::Auto,
                cache_retention: CacheRetention::Short,
                session_id: Some(
                    self.session_manager.lock().await.session_id().to_string(),
                ),
                headers: model.headers.clone(),
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

        AgentLoopConfig {
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
            get_steering_messages: None,
            get_follow_up_messages: None,
            tool_execution: ToolExecutionMode::Parallel,
            pre_tool_use: None,
            post_tool_use: None,
            tools: self.tools.lock().await.clone(),
        }
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
fn should_stop_for_compaction(ctx: &ShouldStopContext, threshold_tokens: u64) -> bool {
    // Sum total tokens from all assistant messages in the conversation
    let total_tokens: u64 = ctx
        .context
        .messages
        .iter()
        .filter_map(|msg| msg.as_standard())
        .filter_map(|msg| match msg {
            Message::Assistant(a) => Some(a.usage.total_tokens),
            _ => None,
        })
        .sum();

    total_tokens >= threshold_tokens
}

/// Build the model_stream function that delegates to rozsa_model's provider registry.
fn build_model_stream_fn() -> ModelStreamFn {
    Box::new(
        |model: &Model,
         context: &rozsa_model::types::Context,
         options: &SimpleStreamOptions| {
            rozsa_model::stream::stream_simple(model, context, options)
        },
    )
}

/// Resolve API key for a model from environment variables.
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

/// Current timestamp in milliseconds since UNIX epoch.
fn current_timestamp_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
