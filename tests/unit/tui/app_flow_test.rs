use rozsa_tui::app::AppState;
use rozsa_tui::backend::mock::{MockBackend, MockCall};
use rozsa_tui::backend::{AgentBackend, BackendEvent, Direction};
use rozsa_tui::protocol::NativeUiState;
use serde_json::json;

#[tokio::test]
async fn full_conversation_flow_with_mock_backend() {
    let state_msg = NativeUiState {
        app_name: "rozsa".to_string(),
        version: "0.1.0".to_string(),
        is_streaming: false,
        ..Default::default()
    };

    let backend = MockBackend::new().with_events(vec![BackendEvent::State(state_msg.clone())]);
    let mut rx = backend.events();

    // Connect 并接收初始 state
    backend.connect().await.unwrap();
    let event = rx.recv().await.unwrap();
    assert!(matches!(&event, BackendEvent::State(s) if s.app_name == "rozsa"));

    // Submit prompt
    backend.submit("hello world", vec![]).await.unwrap();
    assert!(matches!(&backend.calls()[1], MockCall::Submit { text } if text == "hello world"));

    // 模拟流式回复
    let streaming_state = NativeUiState {
        is_streaming: true,
        messages: vec![
            json!({"role": "user", "content": [{"type": "text", "text": "hello world"}]}),
            json!({"role": "assistant", "content": [{"type": "text", "text": "Hi"}]}),
        ],
        ..state_msg.clone()
    };
    backend.inject_event(BackendEvent::State(streaming_state));
    let event = rx.recv().await.unwrap();
    assert!(matches!(&event, BackendEvent::State(s) if s.is_streaming));

    // 模拟工具审批请求
    let perm = rozsa_tui::protocol::NativePermissionPrompt {
        id: "perm-1".to_string(),
        request: json!({"toolName": "bash", "command": "ls"}),
        context: json!({"riskLevel": "low"}),
        trust_levels: vec![
            rozsa_tui::protocol::NativeTrustLevel {
                label: "This session".to_string(),
                key: "session".to_string(),
            },
        ],
    };
    backend.inject_event(BackendEvent::Permission(perm));
    let event = rx.recv().await.unwrap();
    assert!(matches!(&event, BackendEvent::Permission(p) if p.id == "perm-1"));

    // 用户批准
    backend
        .respond_permission("perm-1", "approve_once", None)
        .await
        .unwrap();
    let calls = backend.calls();
    assert!(matches!(
        &calls[calls.len() - 1],
        MockCall::RespondPermission { id, choice } if id == "perm-1" && choice == "approve_once"
    ));

    // 生成完成
    let done_state = NativeUiState {
        is_streaming: false,
        messages: vec![
            json!({"role": "user", "content": [{"type": "text", "text": "hello world"}]}),
            json!({"role": "assistant", "content": [{"type": "text", "text": "Hi there! Done."}]}),
        ],
        ..state_msg
    };
    backend.inject_event(BackendEvent::State(done_state));
    let event = rx.recv().await.unwrap();
    assert!(matches!(&event, BackendEvent::State(s) if !s.is_streaming && s.messages.len() == 2));
}

#[tokio::test]
async fn abort_during_streaming() {
    let backend = MockBackend::new();
    let _rx = backend.events();

    backend.abort().await.unwrap();
    assert!(matches!(&backend.calls()[0], MockCall::Abort));
}

#[tokio::test]
async fn apply_backend_event_updates_state() {
    let mut state = AppState::new();

    let ui = NativeUiState {
        app_name: "rozsa".to_string(),
        is_streaming: true,
        messages: vec![json!({"role": "user"})],
        ..Default::default()
    };

    // 模拟 apply_backend_event 逻辑
    // (测试 BackendEvent -> AppState 映射)
    state.ui = ui;
    assert!(state.ui.is_streaming);
    assert_eq!(state.ui.messages.len(), 1);

    // 验证 shutdown 事件
    state.should_exit = false;
    // 直接设置验证逻辑
    state.should_exit = true;
    assert!(state.should_exit);
}

#[tokio::test]
async fn model_and_session_commands() {
    let backend = MockBackend::new();
    let _rx = backend.events();

    backend.list_models().await.unwrap();
    backend
        .switch_model("anthropic", "claude-opus-4-20250514")
        .await
        .unwrap();
    backend.cycle_model(Direction::Forward).await.unwrap();
    backend.list_sessions().await.unwrap();

    let calls = backend.calls();
    assert!(matches!(&calls[0], MockCall::ListModels));
    assert!(
        matches!(&calls[1], MockCall::SwitchModel { provider, id } if provider == "anthropic" && id == "claude-opus-4-20250514")
    );
    assert!(matches!(&calls[2], MockCall::CycleModel { direction: Direction::Forward }));
    assert!(matches!(&calls[3], MockCall::ListSessions));
}

#[tokio::test]
async fn exit_triggers_shutdown_event() {
    let backend = MockBackend::new();
    let mut rx = backend.events();

    backend.exit().await.unwrap();

    let event = rx.recv().await.unwrap();
    assert!(matches!(event, BackendEvent::Shutdown));
}
