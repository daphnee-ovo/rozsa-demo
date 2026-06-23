use std::path::PathBuf;

use anyhow::Result;

use rozsa_app::agent_session::{AgentSession, AgentSessionConfig};
use rozsa_app::resources::ResourceLoader;
use rozsa_app::session::manager::SessionManager;
use rozsa_app::settings::SettingsManager;
use rozsa_core::events::AgentEvent;
use rozsa_model::types::{
    Api, InputModality, Model, ModelCost, Provider, ThinkingLevel,
};

use crate::args::Args;

pub async fn run(args: &Args) -> Result<()> {
    let cwd = std::env::current_dir()?;

    let home = dirs_next::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let agent_dir = home.join(".rozsa").join("agent");
    let global_settings_path = agent_dir.join("settings.json");
    let project_settings_path = cwd.join(".claude").join("settings.json");

    let settings_manager = SettingsManager::load(
        global_settings_path,
        Some(project_settings_path),
        None,
    )
    .unwrap_or_else(|_| {
        SettingsManager::load(PathBuf::from("/dev/null"), None, None)
            .expect("fallback settings")
    });

    // For now, use a hardcoded model from env or fail
    let model = resolve_model_from_env()?;

    let resource_loader = ResourceLoader::new(cwd.clone(), agent_dir.clone());
    let resources = resource_loader.load().await.unwrap_or_default();
    let system_prompt = ResourceLoader::build_system_prompt(&resources);

    // Create session
    let session_dir = cwd.join(".claude").join("sessions");
    std::fs::create_dir_all(&session_dir)?;
    let session_id = uuid::Uuid::new_v4().to_string();
    let session_path = session_dir.join(format!("{session_id}.jsonl"));

    let session_manager = SessionManager::create(
        &session_path,
        session_id,
        cwd.to_string_lossy().to_string(),
        None,
    )?;

    let config = AgentSessionConfig {
        model,
        thinking_level: ThinkingLevel::Off,
        system_prompt,
        cwd: cwd.clone(),
        session_manager,
        settings_manager,
        resources,
    };

    let mut session = AgentSession::new(config);
    session.register_default_tools(&cwd);

    if let Some(ref prompt) = args.prompt {
        let events = session.prompt(prompt).await?;

        for event in &events {
            if let AgentEvent::MessageEnd { message } = event {
                if let Some(rozsa_model::types::Message::Assistant(msg)) =
                    message.as_standard()
                {
                    for block in &msg.content {
                        if let rozsa_model::types::ContentBlock::Text { text, .. } = block {
                            println!("{text}");
                        }
                    }
                }
            }
        }

        return Ok(());
    }

    // No prompt — launch interactive TUI
    rozsa_tui::app::run_native(session)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))
}

fn resolve_model_from_env() -> Result<Model> {
    // Check for Anthropic
    if std::env::var("ANTHROPIC_API_KEY").is_ok() {
        return Ok(Model {
            id: "claude-sonnet-4-5-20250514".to_string(),
            name: "Claude Sonnet 4.5".to_string(),
            api: Api::AnthropicMessages,
            provider: Provider::Anthropic,
            base_url: "https://api.anthropic.com".to_string(),
            reasoning: true,
            input_modalities: vec![InputModality::Text, InputModality::Image],
            cost: ModelCost { input: 3.0, output: 15.0, cache_read: 0.3, cache_write: 3.75 },
            context_window: 200_000,
            max_tokens: 16_384,
            thinking_level_map: None,
            headers: None,
            compat: None,
        });
    }

    // Check for OpenAI
    if std::env::var("OPENAI_API_KEY").is_ok() {
        return Ok(Model {
            id: "gpt-4o".to_string(),
            name: "GPT-4o".to_string(),
            api: Api::OpenAIResponses,
            provider: Provider::OpenAI,
            base_url: "https://api.openai.com/v1".to_string(),
            reasoning: false,
            input_modalities: vec![InputModality::Text, InputModality::Image],
            cost: ModelCost { input: 2.5, output: 10.0, cache_read: 1.25, cache_write: 0.0 },
            context_window: 128_000,
            max_tokens: 16_384,
            thinking_level_map: None,
            headers: None,
            compat: None,
        });
    }

    anyhow::bail!(
        "No API key found. Set ANTHROPIC_API_KEY or OPENAI_API_KEY to use rozsa."
    )
}
