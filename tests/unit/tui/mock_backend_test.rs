use rozsa_tui::backend::mock::{MockBackend, MockCall};
use rozsa_tui::backend::{AgentBackend, BackendEvent, Direction, ImageData};
use rozsa_tui::protocol::NativeUiState;

#[tokio::test]
async fn mock_backend_submit_records_call() {
    let backend = MockBackend::new();
    let _rx = backend.events();

    backend.submit("hello", vec![]).await.unwrap();
    backend.submit("world", vec![]).await.unwrap();

    let calls = backend.calls();
    assert_eq!(calls.len(), 2);
    assert!(matches!(&calls[0], MockCall::Submit { text } if text == "hello"));
    assert!(matches!(&calls[1], MockCall::Submit { text } if text == "world"));
}

#[tokio::test]
async fn mock_backend_connect_pushes_preset_events() {
    let state = NativeUiState {
        app_name: "rozsa".to_string(),
        version: "0.1.0".to_string(),
        ..Default::default()
    };
    let backend = MockBackend::new().with_events(vec![
        BackendEvent::State(state),
        BackendEvent::Notify {
            level: "info".to_string(),
            message: "connected".to_string(),
        },
    ]);
    let mut rx = backend.events();

    backend.connect().await.unwrap();

    let event1 = rx.recv().await.unwrap();
    assert!(matches!(event1, BackendEvent::State(_)));

    let event2 = rx.recv().await.unwrap();
    assert!(matches!(event2, BackendEvent::Notify { .. }));
}

#[tokio::test]
async fn mock_backend_inject_event() {
    let backend = MockBackend::new();
    let mut rx = backend.events();

    backend.inject_event(BackendEvent::Shutdown);

    let event = rx.recv().await.unwrap();
    assert!(matches!(event, BackendEvent::Shutdown));
}

#[tokio::test]
async fn mock_backend_exit_sends_shutdown() {
    let backend = MockBackend::new();
    let mut rx = backend.events();

    backend.exit().await.unwrap();

    let event = rx.recv().await.unwrap();
    assert!(matches!(event, BackendEvent::Shutdown));
    assert!(matches!(&backend.calls()[0], MockCall::Exit));
}

#[tokio::test]
async fn mock_backend_all_methods_callable() {
    let backend = MockBackend::new();
    let _rx = backend.events();

    backend.connect().await.unwrap();
    backend.abort().await.unwrap();
    backend
        .follow_up("follow", vec![])
        .await
        .unwrap();
    backend.steer("steer", vec![]).await.unwrap();
    backend.list_models().await.unwrap();
    backend.switch_model("openai", "gpt-4").await.unwrap();
    backend.cycle_model(Direction::Forward).await.unwrap();
    backend.list_sessions().await.unwrap();
    backend.switch_session("/tmp/s1").await.unwrap();
    backend.delete_session("/tmp/s2").await.unwrap();
    backend
        .rename_session("/tmp/s3", "new_name")
        .await
        .unwrap();
    backend
        .respond_permission("p1", "approve_once", None)
        .await
        .unwrap();
    backend.run_bash("ls").await.unwrap();
    backend.compact().await.unwrap();
    backend.cycle_edit_mode().await.unwrap();
    backend.switch_agent("sub-1").await.unwrap();
    backend
        .dialog_response("d1", Some("yes"), None, None)
        .await
        .unwrap();
    backend
        .autocomplete_request("/hel", 4, false)
        .await
        .unwrap();
    backend.disconnect().await.unwrap();
    backend.exit().await.unwrap();

    assert_eq!(backend.calls().len(), 20);
}
