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

use anyhow::Result;
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

/// Top-level orchestrator that wires together the agent loop, tools,
/// permissions, extensions, and session persistence.
///
/// This is the main entry point for running a conversation turn.
/// It owns the tools, manages cancellation, and delegates to
/// `rozsa_core::agent_loop` for the actual model interaction loop.
pub struct AgentSession {
    config: AgentSessionConfig,
    tools: Vec<Arc<dyn Tool>>,
    cancel_token: Option<CancellationToken>,
    is_running: bool,
    /// Accumulated messages across turns (the conversation history).
    messages: Vec<AgentMessage>,
}

impl AgentSession {
    /// Create a new agent session from configuration.
    pub fn new(config: AgentSessionConfig) -> Self {
        Self {
            config,
            tools: Vec::new(),
            cancel_token: None,
            is_running: false,
            messages: Vec::new(),
        }
    }

    /// Register a single tool.
    pub fn register_tool(&mut self, tool: Arc<dyn Tool>) {
        self.tools.push(tool);
    }

    /// Register the default built-in tools (read, write, edit, bash, ls, grep, find).
    pub fn register_default_tools(&mut self, cwd: &Path) {
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
        for tool in defaults {
            self.tools.push(Arc::from(tool));
        }
    }

    /// Send a user message and run the agent loop to completion.
    ///
    /// Builds the agent context from current session state, constructs a user
    /// message, runs the core loop, persists resulting messages, and returns
    /// the event stream collected as a Vec.
    pub async fn prompt(&mut self, message: &str) -> Result<Vec<AgentEvent>> {
        if self.is_running {
            anyhow::bail!("Agent session is already running");
        }

        self.is_running = true;
        let cancel_token = CancellationToken::new();
        self.cancel_token = Some(cancel_token.clone());

        // Build user message
        let user_msg = AgentMessage::standard(Message::User(UserMessage {
            content: UserContent::Text(message.to_string()),
            display_text: None,
            timestamp: current_timestamp_ms(),
        }));

        // Persist user message to session file
        self.config
            .session_manager
            .append_message(Message::User(UserMessage {
                content: UserContent::Text(message.to_string()),
                display_text: None,
                timestamp: current_timestamp_ms(),
            }))?;

        // Build context and config for the core loop
        let context = self.build_agent_context();
        let loop_config = self.build_loop_config();

        // Run the agent loop
        let stream = agent_loop(
            vec![user_msg],
            context,
            loop_config,
            Some(cancel_token),
        );

        // Collect all events
        let events = collect_events(stream).await;

        // Persist new assistant/tool messages to session
        self.persist_new_messages(&events)?;

        self.is_running = false;
        self.cancel_token = None;

        Ok(events)
    }

    /// Continue the session without a new user message.
    ///
    /// Used after interruptions or when the model needs to continue
    /// processing (e.g., after compaction).
    pub async fn continue_session(&mut self) -> Result<Vec<AgentEvent>> {
        if self.is_running {
            anyhow::bail!("Agent session is already running");
        }

        self.is_running = true;
        let cancel_token = CancellationToken::new();
        self.cancel_token = Some(cancel_token.clone());

        let context = self.build_agent_context();
        let loop_config = self.build_loop_config();

        let stream = agent_loop_continue(context, loop_config, Some(cancel_token));

        let events = collect_events(stream).await;

        self.persist_new_messages(&events)?;

        self.is_running = false;
        self.cancel_token = None;

        Ok(events)
    }

    /// Abort the currently running loop.
    ///
    /// Signals the CancellationToken, causing the agent loop to terminate
    /// gracefully at the next check point.
    pub fn abort(&mut self) {
        if let Some(token) = &self.cancel_token {
            token.cancel();
        }
        self.is_running = false;
    }

    /// Whether the session is currently running an agent loop.
    pub fn is_running(&self) -> bool {
        self.is_running
    }

    /// Get the current accumulated messages.
    pub fn messages(&self) -> &[AgentMessage] {
        &self.messages
    }

    /// Get a reference to the session manager.
    pub fn session_manager(&self) -> &SessionManager {
        &self.config.session_manager
    }

    /// Get a mutable reference to the session manager.
    pub fn session_manager_mut(&mut self) -> &mut SessionManager {
        &mut self.config.session_manager
    }

    /// Get the settings manager.
    pub fn settings_manager(&self) -> &SettingsManager {
        &self.config.settings_manager
    }

    /// Get the current model.
    pub fn model(&self) -> &Model {
        &self.config.model
    }

    /// Update the model for subsequent turns.
    pub fn set_model(&mut self, model: Model) {
        self.config.model = model;
    }

    /// Update the thinking level for subsequent turns.
    pub fn set_thinking_level(&mut self, level: ThinkingLevel) {
        self.config.thinking_level = level;
    }

    // --- Private helpers ---

    /// Build an AgentContext from the current session state.
    fn build_agent_context(&self) -> AgentContext {
        let tool_schemas: Vec<ToolSchema> = self
            .tools
            .iter()
            .map(|t| ToolSchema {
                name: t.name().to_string(),
                description: t.description().to_string(),
                parameters: t.parameters_schema().clone(),
            })
            .collect();

        AgentContext {
            system_prompt: Some(self.config.system_prompt.clone()),
            messages: self.messages.clone(),
            tools: tool_schemas,
        }
    }

    /// Build the AgentLoopConfig wiring all hooks to this session's dependencies.
    fn build_loop_config(&self) -> AgentLoopConfig {
        let model = self.config.model.clone();
        let thinking_level = self.config.thinking_level;
        let settings = self.config.settings_manager.resolved().clone();

        // Build stream options from settings
        let reasoning = match thinking_level {
            ThinkingLevel::Off => None,
            level => Some(level),
        };

        let stream_options = SimpleStreamOptions {
            base: StreamOptions {
                temperature: None,
                max_tokens: Some(model.max_tokens),
                api_key: None,
                transport: Transport::Auto,
                cache_retention: CacheRetention::Short,
                session_id: Some(
                    self.config.session_manager.session_id().to_string(),
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
            before_tool_call: None,
            after_tool_call: None,
            tools: self.tools.clone(),
        }
    }

    /// Persist new messages from agent events into the session file and internal history.
    fn persist_new_messages(&mut self, events: &[AgentEvent]) -> Result<()> {
        for event in events {
            if let AgentEvent::AgentEnd { messages } = event {
                for msg in messages {
                    // Add to in-memory history
                    self.messages.push(msg.clone());

                    // Persist standard messages to session file
                    if let Some(message) = msg.as_standard() {
                        self.config.session_manager.append_message(message.clone())?;
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

/// Collect all events from an EventStream into a Vec.
async fn collect_events(mut stream: EventStream<AgentEvent>) -> Vec<AgentEvent> {
    let mut events = Vec::new();
    while let Some(event) = stream.next().await {
        events.push(event);
    }
    events
}

/// Current timestamp in milliseconds since UNIX epoch.
fn current_timestamp_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
