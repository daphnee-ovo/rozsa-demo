use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use dashmap::DashMap;

use rozsa_app::agent_session::{AgentSession, AgentSessionConfig};
use rozsa_app::model_registry::ModelRegistry;
use rozsa_app::permissions::{
    ApprovalInfo, PendingApprovals, PermissionMode, PermissionPolicy, PermissionResponse,
    PolicyVerdict,
};
use rozsa_app::resources::ResourceLoader;
use rozsa_app::session::manager::SessionManager;
use rozsa_app::settings::SettingsManager;
use rozsa_core::events::AgentEvent;

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

    let session_manager = SessionManager::create_lazy(
        &session_path,
        session_id,
        cwd.to_string_lossy().to_string(),
        None,
    );

    let thinking_level = settings_manager.default_thinking_level_parsed();

    // Permission system setup
    let permission_mode = PermissionMode::parse(&settings_manager.resolved().permissions.mode)
        .unwrap_or(PermissionMode::OnRequest);

    let auto_approve_patterns = settings_manager
        .resolved()
        .permissions
        .auto_approve_patterns
        .clone();

    let mut policy = PermissionPolicy::new(permission_mode, auto_approve_patterns);

    // Persist session approvals to settings file for cross-session reuse.
    let settings_for_persist = Arc::new(std::sync::Mutex::new(settings_manager.clone()));
    {
        let settings_for_cb = settings_for_persist.clone();
        policy.set_on_approval(Box::new(move |trust_key| {
            if let Ok(mut mgr) = settings_for_cb.lock() {
                mgr.add_trusted_pattern(trust_key);
            }
        }));
    }

    let policy = Arc::new(policy);
    let pending_approvals: PendingApprovals = Arc::new(DashMap::new());
    let (perm_req_tx, perm_req_rx) =
        tokio::sync::mpsc::unbounded_channel::<(String, ApprovalInfo)>();

    let pre_tool_use_hook = {
        let policy = policy.clone();
        let pending = pending_approvals.clone();
        let perm_req_tx = perm_req_tx.clone();
        let hook: Box<
            dyn Fn(
                    rozsa_core::config::PreToolUseContext,
                )
                    -> std::pin::Pin<Box<dyn std::future::Future<Output = Option<rozsa_core::config::PreToolUseResult>> + Send>>
                + Send
                + Sync,
        > = Box::new(move |ctx| {
            let policy = policy.clone();
            let pending = pending.clone();
            let perm_req_tx = perm_req_tx.clone();
            Box::pin(async move {
                let verdict = policy.evaluate(&ctx.tool_name, &ctx.args);
                match verdict {
                    PolicyVerdict::Allow => None,
                    PolicyVerdict::Block { reason } => {
                        Some(rozsa_core::config::PreToolUseResult {
                            block: true,
                            reason: Some(reason),
                        })
                    }
                    PolicyVerdict::NeedApproval { info } => {
                        let (tx, rx) = tokio::sync::oneshot::channel();
                        let request_id = ctx.tool_call_id.clone();
                        pending.insert(request_id.clone(), tx);
                        let _ = perm_req_tx.send((request_id, info));

                        match rx.await {
                            Ok(PermissionResponse::Allow) => None,
                            Ok(PermissionResponse::AllowSession { trust_key }) => {
                                policy.record_session_approval(trust_key);
                                None
                            }
                            Ok(PermissionResponse::Deny) | Err(_) => {
                                Some(rozsa_core::config::PreToolUseResult {
                                    block: true,
                                    reason: Some("Permission denied by user".to_string()),
                                })
                            }
                        }
                    }
                }
            })
        });
        hook
    };

    let config = AgentSessionConfig {
        model,
        thinking_level,
        system_prompt,
        cwd: cwd.clone(),
        session_manager,
        settings_manager,
        resources,
        pre_tool_use: Some(pre_tool_use_hook),
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
            pending_approvals: Some(pending_approvals),
            permission_request_rx: Some(perm_req_rx),
        },
    )
    .await
    .map_err(|e| anyhow::anyhow!("{e}"))
}

