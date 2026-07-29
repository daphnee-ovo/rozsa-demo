// FrameworkTree
// run.rs
// ├── run()
// └── check_version()

use std::sync::Arc;

use anyhow::Result;
use dashmap::DashMap;

use rozsa_app::agent_session::{AgentSession, AgentSessionConfig};
use rozsa_app::config_paths::ConfigRoots;
use rozsa_app::model_registry::ModelRegistry;
use rozsa_app::permissions::{
    PendingApprovals, PermissionController, PermissionMode, PermissionResponse, PolicyVerdict,
};
use rozsa_app::resources::ResourceLoader;
use rozsa_app::session::manager::SessionManager;
use rozsa_app::settings::SettingsManager;
use rozsa_app::tools::AskUserQuestionRequest;
use rozsa_core::events::AgentEvent;

use crate::args::Args;

pub async fn run(args: &Args) -> Result<()> {
    rozsa_model::providers::register_builtin_providers();

    let process_cwd = std::env::current_dir()?;
    let (cwd, prompt) =
        crate::args::resolve_positional_input(args.prompt.as_deref(), &process_cwd, args.print)?;

    let config_roots = ConfigRoots::discover(&cwd)?;
    let [global_settings_path, project_settings_path] = config_roots.settings_paths();

    let settings_manager = SettingsManager::load(
        global_settings_path.clone(),
        Some(project_settings_path),
        None,
    )?;

    // Spawn non-blocking version check
    if std::env::var("ROZSA_SKIP_VERSION_CHECK").unwrap_or_default() != "1" {
        tokio::spawn(async {
            check_version().await;
        });
    }

    // Resolve model from registry: user-level then project-level (project overrides user)
    let [user_models_dir, project_models_dir] = config_roots.model_dirs();
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
            if prompt.is_some() || args.print {
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

    let resource_loader = ResourceLoader::new(cwd.clone(), config_roots.resource_dirs().to_vec());
    let resources = resource_loader.load().await?;
    let system_prompt = if let Some(ref custom_prompt) = args.system_prompt {
        // Use --system-prompt to override
        custom_prompt.clone()
    } else {
        ResourceLoader::build_system_prompt(&resources)
    };

    // Global sessions are writable by default; the project layer can override
    // matching session ids when both layers are read.
    let session_dirs = config_roots.session_dirs(&cwd).to_vec();
    let session_dir = config_roots.writable_session_dir(&cwd);
    std::fs::create_dir_all(&session_dir)?;

    // Resolve the parent session path for GUI or direct continuation. The
    // consumer copies the parent's persisted context into the new branch.
    let initial_parent_session = if args.continue_session {
        // Find the most recent session file
        let sessions = SessionManager::list_dirs(&session_dirs)?;
        if let Some(latest) = sessions.first() {
            Some(latest.path.to_string_lossy().to_string())
        } else {
            anyhow::bail!("No previous session found to continue");
        }
    } else if let Some(ref resume_id) = args.resume {
        // Resume a specific session by ID
        let existing_path = session_dirs
            .iter()
            .rev()
            .map(|dir| dir.join(format!("{resume_id}.jsonl")))
            .find(|path| path.exists())
            .ok_or_else(|| anyhow::anyhow!("Session {resume_id} not found"))?;
        Some(existing_path.to_string_lossy().to_string())
    } else {
        None
    };

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

    let permission_controller = Arc::new(PermissionController::with_project_rules(
        permission_mode,
        settings_manager.resolved().permissions.deny.clone(),
        settings_manager.resolved().permissions.ask.clone(),
        settings_manager.resolved().permissions.allow.clone(),
        cwd.clone(),
        settings_manager.clone(),
    ));
    let pending_approvals: PendingApprovals = Arc::new(DashMap::new());
    let (perm_req_tx, perm_req_rx) =
        tokio::sync::mpsc::unbounded_channel::<rozsa_gui::state::PermissionRequest>();
    let (question_req_tx, question_req_rx) =
        tokio::sync::mpsc::unbounded_channel::<AskUserQuestionRequest>();

    let pre_tool_use_factory: rozsa_gui::state::PreToolUseHookFactory = {
        let controller = permission_controller.clone();
        let pending = pending_approvals.clone();
        let perm_req_tx = perm_req_tx.clone();
        Arc::new(move |session_id| {
            let controller = controller.clone();
            let pending = pending.clone();
            let perm_req_tx = perm_req_tx.clone();
            Arc::new(move |ctx| {
                let controller = controller.clone();
                let pending = pending.clone();
                let perm_req_tx = perm_req_tx.clone();
                let session_id = session_id.clone();
                Box::pin(async move {
                    let verdict = controller.evaluate(&session_id, &ctx.tool_name, &ctx.args);
                    match verdict {
                        PolicyVerdict::Allow => None,
                        PolicyVerdict::Block { reason } => {
                            Some(rozsa_core::config::PreToolUseResult {
                                block: true,
                                reason: Some(reason),
                            })
                        }
                        PolicyVerdict::NeedApproval { info } => {
                            let description = ctx
                                .context
                                .tools
                                .iter()
                                .find(|tool| tool.name == ctx.tool_name)
                                .map(|tool| tool.description.clone())
                                .unwrap_or_default();
                            let (tx, rx) = tokio::sync::oneshot::channel();
                            let request_id = ctx.tool_call_id.clone();
                            let pending_key =
                                rozsa_gui::state::permission_pending_key(&session_id, &request_id);
                            pending.insert(pending_key.clone(), tx);
                            let _ = perm_req_tx.send(rozsa_gui::state::PermissionRequest {
                                session_id: session_id.clone(),
                                turn_id: ctx
                                    .assistant_message
                                    .response_id
                                    .clone()
                                    .unwrap_or_else(|| ctx.tool_call_id.clone()),
                                request_id,
                                tool_name: ctx.tool_name.clone(),
                                description,
                                args: ctx.args.clone(),
                                info,
                            });

                            let response = match ctx.signal {
                                Some(signal) => {
                                    tokio::select! {
                                        biased;
                                        _ = signal.cancelled() => {
                                            pending.remove(&pending_key);
                                            return Some(rozsa_core::config::PreToolUseResult {
                                                block: true,
                                                reason: Some("Permission request cancelled".to_string()),
                                            });
                                        }
                                        response = rx => response,
                                    }
                                }
                                None => rx.await,
                            };
                            pending.remove(&pending_key);
                            match response {
                                Ok(PermissionResponse::Allow) => None,
                                Ok(PermissionResponse::AllowSession { trust_key }) => {
                                    if let Err(error) =
                                        controller.record_project_approval(&trust_key)
                                    {
                                        return Some(rozsa_core::config::PreToolUseResult {
                                            block: true,
                                            reason: Some(format!(
                                                "Failed to persist project trust: {error}"
                                            )),
                                        });
                                    }
                                    None
                                }
                                Ok(PermissionResponse::Deny) | Err(_) => {
                                    Some(rozsa_core::config::PreToolUseResult {
                                        block: true,
                                        reason: Some("Permission denied by user".to_string()),
                                    })
                                }
                                Ok(PermissionResponse::DenyWithHint { hint }) => {
                                    Some(rozsa_core::config::PreToolUseResult {
                                        block: true,
                                        reason: Some(format!("Permission denied by user. {hint}")),
                                    })
                                }
                            }
                        }
                    }
                })
            })
        })
    };

    if args.print && prompt.is_none() {
        anyhow::bail!("--print requires a prompt argument");
    }

    if prompt.is_none() {
        return rozsa_gui::run(rozsa_gui::GuiConfig {
            initial_parent_session,
            model,
            thinking_level,
            cwd,
            config_roots,
            settings_manager,
            model_registry: Some(Arc::new(registry)),
            session_dir,
            session_dirs,
            global_settings_path: Some(global_settings_path),
            pending_approvals: Some(pending_approvals),
            permission_request_rx: Some(perm_req_rx),
            question_request_tx: Some(question_req_tx),
            question_request_rx: Some(question_req_rx),
            permission_controller,
            pre_tool_use_factory: Some(pre_tool_use_factory),
            system_prompt,
            resources,
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"));
    }

    let session_id = uuid::Uuid::new_v4().to_string();
    let session_path = session_dir.join(format!("{session_id}.jsonl"));
    let parent_session = initial_parent_session.clone();
    let mut session_manager = SessionManager::create_lazy(
        &session_path,
        session_id.clone(),
        cwd.to_string_lossy().to_string(),
        initial_parent_session,
    );
    if let Some(parent_path) = parent_session {
        session_manager.copy_context_messages_from_path(parent_path)?;
    }

    let config = AgentSessionConfig {
        model,
        thinking_level,
        system_prompt,
        cwd: cwd.clone(),
        session_manager,
        settings_manager,
        resources,
        pre_tool_use: Some({
            let hook = pre_tool_use_factory(session_id.clone());
            Box::new(move |context| hook(context))
        }),
        model_stream: None,
    };

    let session = AgentSession::new(config);
    session.register_default_tools(&cwd).await;

    if let Some(ref prompt) = prompt {
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

    anyhow::bail!("No interactive frontend selected")
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
