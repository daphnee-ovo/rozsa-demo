// 回归测试：`!command` bang escape 被 NativeBackend 拦截执行，不转发给 agent。
// 修复前 `!` 前缀未检测，直接当作普通 prompt 发给 session.prompt()。
// 现在验证：bash 输出以 bashExecution 消息渲染到对话区（State 事件）。

use rozsa_app::agent_session::{AgentSession, AgentSessionConfig};
use rozsa_app::resources::LoadedResources;
use rozsa_app::session::manager::SessionManager;
use rozsa_app::settings::SettingsManager;
use rozsa_model::types::{Api, InputModality, Model, ModelCost, Provider, ThinkingLevel};
use rozsa_tui::backend::native::NativeBackend;
use rozsa_tui::backend::{AgentBackend, BackendEvent};

fn test_model() -> Model {
    Model {
        id: "test-model".to_string(),
        name: "Test Model".to_string(),
        api: Api::OpenAIResponses,
        provider: Provider::OpenAI,
        base_url: "https://example.invalid".to_string(),
        reasoning: false,
        input_modalities: vec![InputModality::Text],
        cost: ModelCost {
            input: 0.0,
            output: 0.0,
            cache_read: 0.0,
            cache_write: 0.0,
        },
        context_window: 8192,
        max_tokens: 2048,
        thinking_level_map: None,
        headers: None,
        compat: None,
    }
}

fn create_backend(tmp_dir: &tempfile::TempDir) -> NativeBackend {
    let session_path = tmp_dir.path().join("session.jsonl");
    let session_manager = SessionManager::create(
        &session_path,
        "test-session".to_string(),
        tmp_dir.path().to_string_lossy().to_string(),
        None,
    )
    .unwrap();

    let settings_manager =
        SettingsManager::load(tmp_dir.path().join("global-settings.json"), None, None).unwrap();

    let session = AgentSession::new(AgentSessionConfig {
        model: test_model(),
        thinking_level: ThinkingLevel::Off,
        system_prompt: "test".to_string(),
        cwd: tmp_dir.path().to_path_buf(),
        session_manager,
        settings_manager,
        resources: LoadedResources::default(),
        pre_tool_use: None,
    });

    NativeBackend::new(session)
}

/// 等待收到包含已完成 bashExecution 消息（exitCode 非 null）的 State 事件。
/// Returns the custom message's payload Value.
async fn wait_for_bash_state(
    rx: &mut tokio::sync::mpsc::UnboundedReceiver<BackendEvent>,
) -> serde_json::Value {
    use rozsa_core::messages::AgentMessage;
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        loop {
            match rx.recv().await {
                Some(BackendEvent::State(state)) => {
                    for msg in &state.messages {
                        if let AgentMessage::Custom { message } = msg {
                            if message.message_type == "bashExecution"
                                && !message.payload.get("exitCode").is_some_and(|v| v.is_null())
                            {
                                return message.payload.clone();
                            }
                        }
                    }
                    continue;
                }
                Some(_) => continue,
                None => panic!("event channel closed"),
            }
        }
    })
    .await
    .expect("should receive State with completed bashExecution within timeout")
}

/// `!echo hello` 应被拦截执行，产生 bashExecution 消息渲染到对话区。
#[tokio::test]
async fn bang_command_renders_in_conversation() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let backend = create_backend(&tmp_dir);
    let mut rx = backend.events();

    backend
        .submit("!echo bang_test_marker", vec![])
        .await
        .unwrap();

    let msg = wait_for_bash_state(&mut rx).await;
    let command = msg.get("command").and_then(|v| v.as_str()).unwrap_or("");
    let output = msg.get("output").and_then(|v| v.as_str()).unwrap_or("");

    assert_eq!(command, "echo bang_test_marker");
    assert!(
        output.contains("bang_test_marker"),
        "output should contain command result, got: {output}"
    );
}

/// `!!echo hello` (双叹号) 同样应被拦截并渲染。
#[tokio::test]
async fn double_bang_command_renders_in_conversation() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let backend = create_backend(&tmp_dir);
    let mut rx = backend.events();

    backend
        .submit("!!echo double_bang_marker", vec![])
        .await
        .unwrap();

    let msg = wait_for_bash_state(&mut rx).await;
    let command = msg.get("command").and_then(|v| v.as_str()).unwrap_or("");
    let output = msg.get("output").and_then(|v| v.as_str()).unwrap_or("");

    assert_eq!(command, "echo double_bang_marker");
    assert!(
        output.contains("double_bang_marker"),
        "output should contain command result, got: {output}"
    );
}

/// 空 `!` 应被拦截但不执行任何命令。
#[tokio::test]
async fn empty_bang_is_noop() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let backend = create_backend(&tmp_dir);
    let mut rx = backend.events();

    backend.submit("!", vec![]).await.unwrap();

    // 短暂等待后应无 bashExecution 消息
    let result = tokio::time::timeout(std::time::Duration::from_millis(200), rx.recv()).await;

    assert!(result.is_err(), "empty bang should not produce any events");
}
