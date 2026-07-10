// 回归测试：/thinking 和 /model 命令修改 settings 后持久化到 global settings 文件。
// 修复前只修改运行时状态，不写文件，重启后丢失。

use rozsa_app::agent_session::{AgentSession, AgentSessionConfig};
use rozsa_app::resources::LoadedResources;
use rozsa_app::session::manager::SessionManager;
use rozsa_app::settings::SettingsManager;
use rozsa_model::types::{Api, InputModality, Model, ModelCost, Provider, ThinkingLevel};
use rozsa_tui::backend::AgentBackend;
use rozsa_tui::backend::native::{NativeBackend, NativeBackendConfig};
use std::path::PathBuf;

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

fn create_backend(tmp_dir: &tempfile::TempDir) -> (NativeBackend, PathBuf) {
    let session_path = tmp_dir.path().join("session.jsonl");
    let session_manager = SessionManager::create(
        &session_path,
        "test-session".to_string(),
        tmp_dir.path().to_string_lossy().to_string(),
        None,
    )
    .unwrap();

    let global_settings_path = tmp_dir.path().join("settings.json");
    let settings_manager = SettingsManager::load(global_settings_path.clone(), None, None).unwrap();

    let session = AgentSession::new(AgentSessionConfig {
        model: test_model(),
        thinking_level: ThinkingLevel::Off,
        system_prompt: "test".to_string(),
        cwd: tmp_dir.path().to_path_buf(),
        session_manager,
        settings_manager,
        resources: LoadedResources::default(),
        pre_tool_use: None,
        model_stream: None,
    });

    let config = NativeBackendConfig {
        model_registry: None,
        session_dir: None,
        global_settings_path: Some(global_settings_path.clone()),
        pending_approvals: None,
        permission_request_rx: None,
    };

    (
        NativeBackend::with_config(session, config),
        global_settings_path,
    )
}

fn read_settings_json(path: &std::path::Path) -> serde_json::Value {
    let content = std::fs::read_to_string(path).unwrap_or_default();
    serde_json::from_str(&content).unwrap_or(serde_json::Value::Null)
}

/// /thinking 命令应持久化 defaultThinkingLevel 到 settings 文件。
#[tokio::test]
async fn thinking_command_persists_to_settings_file() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let (backend, settings_path) = create_backend(&tmp_dir);
    let _rx = backend.events();

    // 初始无 settings 文件
    assert!(!settings_path.exists());

    backend.submit("/thinking medium", vec![]).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let settings = read_settings_json(&settings_path);
    assert_eq!(
        settings
            .get("defaultThinkingLevel")
            .and_then(|v| v.as_str()),
        Some("medium"),
        "settings file should contain defaultThinkingLevel=medium"
    );
}

/// /thinking off 也应正确持久化。
#[tokio::test]
async fn thinking_off_persists() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let (backend, settings_path) = create_backend(&tmp_dir);
    let _rx = backend.events();

    backend.submit("/thinking high", vec![]).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    backend.submit("/thinking off", vec![]).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let settings = read_settings_json(&settings_path);
    assert_eq!(
        settings
            .get("defaultThinkingLevel")
            .and_then(|v| v.as_str()),
        Some("off"),
    );
}

/// Settings dialog 的 cycle_setting（通过 update_setting "__cycle_setting"）也应持久化。
/// index 0 = thinking level, direction 1 = next (Off → Low)
#[tokio::test]
async fn settings_dialog_cycle_thinking_persists() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let (backend, settings_path) = create_backend(&tmp_dir);
    let _rx = backend.events();

    // cycle thinking: Off → Low (index=0, direction=1)
    backend
        .update_setting("__cycle_setting", "0:1")
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let settings = read_settings_json(&settings_path);
    assert_eq!(
        settings
            .get("defaultThinkingLevel")
            .and_then(|v| v.as_str()),
        Some("low"),
        "cycle_setting should persist thinking level"
    );

    // cycle again: Low → Medium
    backend
        .update_setting("__cycle_setting", "0:1")
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let settings = read_settings_json(&settings_path);
    assert_eq!(
        settings
            .get("defaultThinkingLevel")
            .and_then(|v| v.as_str()),
        Some("medium"),
    );
}

/// Settings dialog 切换 transport 也应持久化。
#[tokio::test]
async fn settings_dialog_cycle_transport_persists() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let (backend, settings_path) = create_backend(&tmp_dir);
    let _rx = backend.events();

    // cycle transport: auto → sse (index=4, direction=1)
    backend
        .update_setting("__cycle_setting", "4:1")
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let settings = read_settings_json(&settings_path);
    assert_eq!(
        settings.get("transport").and_then(|v| v.as_str()),
        Some("sse"),
        "cycle_setting should persist transport"
    );
}
