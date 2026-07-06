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
        SettingsManager::load(PathBuf::from("/dev/null"), None, None).expect("fallback settings")
    });

    // Spawn non-blocking version check
    if std::env::var("ROZSA_SKIP_VERSION_CHECK").unwrap_or_default() != "1" {
        tokio::spawn(async {
            check_version().await;
        });
    }

    // Resolve model from registry: user-level then project-level (project overrides user)
    let user_models_dir = home.join(".rozsa").join("models");
    let project_models_dir = cwd.join(".rozsa").join("models");
    let registry = ModelRegistry::load_from_dirs(&[&user_models_dir, &project_models_dir])?;

    let model = if let Some(ref model_arg) = args.model {
        registry.find_by_id(model_arg)
    } else if let Some(ref provider_arg) = args.provider {
        // Use --provider to override provider selection
        if let Some(model_id) = settings_manager.default_model() {
            registry.resolve(provider_arg, model_id)
        } else {
            registry.first_available()
        }
    } else if let (Some(provider), Some(model_id)) = (
        settings_manager.default_provider(),
        settings_manager.default_model(),
    ) {
        registry.resolve(provider, model_id)
    } else {
        registry.first_available()
    };

    let model = match model {
        Some(m) => m,
        None => {
            // prompt/print 模式必须有模型
            if args.prompt.is_some() || args.print {
                anyhow::bail!(
                    "No model available. Add model configs to ~/.rozsa/models/ or <project>/.rozsa/models/, or specify --model."
                );
            }
            // GUI 模式允许无模型启动 — 使用 placeholder，用户可在 GUI 设置中配置
            rozsa_model::types::Model {
                id: "(unconfigured)".to_string(),
                name: "No model configured".to_string(),
                api: rozsa_model::types::Api::OpenAICompletions,
                provider: rozsa_model::types::Provider::OpenAI,
                base_url: String::new(),
                reasoning: false,
                input_modalities: vec![],
                cost: rozsa_model::types::ModelCost {
                    input: 0.0,
                    output: 0.0,
                    cache_read: 0.0,
                    cache_write: 0.0,
                },
                context_window: 128000,
                max_tokens: 16384,
                thinking_level_map: None,
                headers: None,
                compat: None,
            }
        }
    };

    let resource_loader = ResourceLoader::new(cwd.clone(), agent_dir.clone());
    let resources = resource_loader.load().await.unwrap_or_default();
    let system_prompt = if let Some(ref custom_prompt) = args.system_prompt {
        // Use --system-prompt to override
        custom_prompt.clone()
    } else {
        ResourceLoader::build_system_prompt(&resources)
    };

    // Session 存储在 ~/.rozsa/agent/sessions/<cwd-encoded>/
    let cwd_encoded = cwd
        .to_string_lossy()
        .replace('/', "-")
        .trim_matches('-')
        .to_string();
    let session_dir = agent_dir.join("sessions").join(format!("-{cwd_encoded}-"));
    std::fs::create_dir_all(&session_dir)?;

    // Handle --continue and --resume
    let (session_id, session_path, parent_id) = if args.continue_session {
        // Find the most recent session file
        let mut entries: Vec<_> = std::fs::read_dir(&session_dir)?
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path()
                    .extension()
                    .and_then(|s| s.to_str())
                    .map(|s| s == "jsonl")
                    .unwrap_or(false)
            })
            .collect();
        entries.sort_by_key(|e| e.metadata().ok().and_then(|m| m.modified().ok()));

        if let Some(last_entry) = entries.last() {
            let path = last_entry.path();
            let old_session_id = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            let new_session_id = uuid::Uuid::new_v4().to_string();
            let new_path = session_dir.join(format!("{new_session_id}.jsonl"));
            (new_session_id, new_path, Some(old_session_id))
        } else {
            anyhow::bail!("No previous session found to continue");
        }
    } else if let Some(ref resume_id) = args.resume {
        // Resume a specific session by ID
        let existing_path = session_dir.join(format!("{resume_id}.jsonl"));
        if !existing_path.exists() {
            anyhow::bail!("Session {} not found", resume_id);
        }
        let new_session_id = uuid::Uuid::new_v4().to_string();
        let new_path = session_dir.join(format!("{new_session_id}.jsonl"));
        (new_session_id, new_path, Some(resume_id.clone()))
    } else {
        // Create new session
        let session_id = uuid::Uuid::new_v4().to_string();
        let session_path = session_dir.join(format!("{session_id}.jsonl"));
        (session_id, session_path, None)
    };

    let session_manager = SessionManager::create_lazy(
        &session_path,
        session_id,
        cwd.to_string_lossy().to_string(),
        parent_id,
    );

    let thinking_level = if let Some(ref thinking_arg) = args.thinking {
        // Parse --thinking argument
        match thinking_arg.to_lowercase().as_str() {
            "off" => rozsa_model::types::ThinkingLevel::Off,
            "minimal" => rozsa_model::types::ThinkingLevel::Minimal,
            "low" => rozsa_model::types::ThinkingLevel::Low,
            "medium" => rozsa_model::types::ThinkingLevel::Medium,
            "high" => rozsa_model::types::ThinkingLevel::High,
            "xhigh" => rozsa_model::types::ThinkingLevel::XHigh,
            _ => anyhow::bail!(
                "Invalid thinking level: {}. Valid values: off, minimal, low, medium, high, xhigh",
                thinking_arg
            ),
        }
    } else {
        settings_manager.default_thinking_level_parsed()
    };

    // Permission system setup
    let permission_mode = PermissionMode::parse(&settings_manager.resolved().permissions.mode)
        .unwrap_or(PermissionMode::OnRequest);

    let auto_approve_patterns = settings_manager
        .resolved()
        .permissions
        .auto_approve_patterns
        .clone();

    let allowed_tools = settings_manager
        .resolved()
        .permissions
        .allowed_tools
        .clone();

    let blocked_commands = settings_manager
        .resolved()
        .permissions
        .blocked_commands
        .clone();

    let mut policy = PermissionPolicy::new(
        permission_mode,
        auto_approve_patterns,
        allowed_tools,
        blocked_commands,
    );

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
                ) -> std::pin::Pin<
                    Box<
                        dyn std::future::Future<
                                Output = Option<rozsa_core::config::PreToolUseResult>,
                            > + Send,
                    >,
                > + Send
                + Sync,
        > = Box::new(move |ctx| {
            let policy = policy.clone();
            let pending = pending.clone();
            let perm_req_tx = perm_req_tx.clone();
            Box::pin(async move {
                let verdict = policy.evaluate(&ctx.tool_name, &ctx.args);
                match verdict {
                    PolicyVerdict::Allow => None,
                    PolicyVerdict::Block { reason } => Some(rozsa_core::config::PreToolUseResult {
                        block: true,
                        reason: Some(reason),
                    }),
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

    let gui_system_prompt = system_prompt.clone();
    let gui_resources = resources.clone();

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

    if args.print && args.prompt.is_none() {
        anyhow::bail!("--print requires a prompt argument");
    }

    if let Some(ref prompt) = args.prompt {
        let events = session.prompt(prompt).await?;

        match args.output_format {
            crate::args::OutputFormat::Text => {
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
            }
            crate::args::OutputFormat::Json => {
                for event in &events {
                    let json = serde_json::to_string(event).unwrap_or_else(|_| "{}".to_string());
                    println!("{json}");
                }
            }
        }

        return Ok(());
    }

    // No prompt — launch interactive GUI (default) or TUI (--tui flag)
    #[cfg(feature = "tui")]
    if args.tui {
        return rozsa_tui::app::run_native_with(
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
        .map_err(|e| anyhow::anyhow!("{e}"));
    }

    #[cfg(not(feature = "tui"))]
    if args.tui {
        anyhow::bail!("TUI mode not available. Rebuild with --features tui");
    }

    rozsa_gui::run(rozsa_gui::GuiConfig {
        session,
        model_registry: Some(Arc::new(registry)),
        session_dir: Some(session_dir),
        global_settings_path: Some(global_settings_path),
        pending_approvals: Some(pending_approvals),
        permission_request_rx: Some(perm_req_rx),
        system_prompt: gui_system_prompt,
        resources: gui_resources,
    })
    .await
    .map_err(|e| anyhow::anyhow!("{e}"))
}

async fn check_version() {
    let Ok(resp) = reqwest::get("https://rozsa.dev/api/latest-version").await else {
        return;
    };
    let Ok(text) = resp.text().await else {
        return;
    };
    let latest = text.trim();
    let current = env!("CARGO_PKG_VERSION");
    if latest != current && !latest.is_empty() {
        eprintln!(
            "\x1b[33mNew version available: {} (current: {})\x1b[0m",
            latest, current
        );
    }
}
