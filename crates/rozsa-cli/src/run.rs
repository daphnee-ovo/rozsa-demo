use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;

use rozsa_app::agent_session::{AgentSession, AgentSessionConfig};
use rozsa_app::model_registry::ModelRegistry;
use rozsa_app::resources::ResourceLoader;
use rozsa_app::session::manager::SessionManager;
use rozsa_app::settings::SettingsManager;
use rozsa_core::events::AgentEvent;
use rozsa_model::types::ThinkingLevel;

use crate::args::Args;

pub async fn run(args: &Args) -> Result<()> {
    rozsa_model::providers::register_builtin_providers();

    let cwd = std::env::current_dir()?;

    let home = dirs_next::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let agent_dir = home.join(".rozsa").join("agent");
    let global_settings_path = agent_dir.join("settings.json");
    let project_settings_path = cwd.join(".claude").join("settings.json");

    let settings_manager = SettingsManager::load(
        global_settings_path.clone(),
        Some(project_settings_path),
        None,
    )
    .unwrap_or_else(|_| {
        SettingsManager::load(PathBuf::from("/dev/null"), None, None)
            .expect("fallback settings")
    });

    // Resolve model from registry (reads generated models + models.json + env API keys)
    let models_json_path = agent_dir.join("models.json");
    let registry = ModelRegistry::from_generated_with_models_json_path(
        Some(&models_json_path),
    )?;

    let model = if let Some(ref model_arg) = args.model {
        registry.find_by_id(model_arg)
    } else if let (Some(provider), Some(model_id)) = (
        settings_manager.default_provider(),
        settings_manager.default_model(),
    ) {
        registry.resolve(provider, model_id)
    } else {
        registry.first_available()
    };

    let Some(model) = model else {
        anyhow::bail!(
            "No model available. Configure a provider API key (ANTHROPIC_API_KEY, OPENAI_API_KEY, etc.) or specify --model."
        );
    };

    let resource_loader = ResourceLoader::new(cwd.clone(), agent_dir.clone());
    let resources = resource_loader.load().await.unwrap_or_default();
    let system_prompt = ResourceLoader::build_system_prompt(&resources);

    // Session 存储在 ~/.rozsa/agent/sessions/<cwd-encoded>/
    let cwd_encoded = cwd
        .to_string_lossy()
        .replace('/', "-")
        .trim_matches('-')
        .to_string();
    let session_dir = agent_dir.join("sessions").join(format!("-{cwd_encoded}-"));
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
        pre_tool_use: None,
    };

    let session = AgentSession::new(config);
    session.register_default_tools(&cwd).await;

    if let Some(ref prompt) = args.prompt {
        let events = session.prompt(prompt).await?;

        for event in &events {
            if let AgentEvent::MessageEnd { message } = event {
                if let Some(rozsa_model::types::Message::Assistant(assistant)) =
                    message.as_standard()
                {
                    for block in &assistant.content {
                        if let rozsa_model::types::ContentBlock::Text { text, .. } = block {
                            print!("{text}");
                        }
                    }
                    println!();
                }
            }
        }

        return Ok(());
    }

    // No prompt — launch interactive TUI
    rozsa_tui::app::run_native_with(
        session,
        rozsa_tui::backend::native::NativeBackendConfig {
            model_registry: Some(Arc::new(registry)),
            session_dir: Some(session_dir),
            global_settings_path: Some(global_settings_path),
        },
    )
    .await
    .map_err(|e| anyhow::anyhow!("{e}"))
}

