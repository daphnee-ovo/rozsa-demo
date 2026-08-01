// FrameworkTree
// model_switch_test.rs
// ├── function_body()
// ├── model_switch_snapshots_agents_before_await()
// ├── model_switch_uses_consistent_settings_lock_order()
// └── slash_model_switch_publishes_main_state()

fn function_body<'a>(source: &'a str, signature: &str, next_signature: &str) -> &'a str {
    let start = source.find(signature).expect("function must exist");
    let body = &source[start..];
    let end = body
        .find(next_signature)
        .expect("next function boundary must exist");
    &body[..end]
}

#[test]
fn model_switch_snapshots_agents_before_await() {
    let source = include_str!("../src/commands.rs");
    let body = function_body(source, "pub async fn switch_model(", "// --- 认证 ---");

    let snapshot = body
        .find("let agents = {")
        .expect("agents must be snapshotted");
    let await_set_model = body
        .find("agent.set_model(model.clone()).await")
        .expect("agent model update must remain asynchronous");
    assert!(snapshot < await_set_model);
    assert!(body[..await_set_model].contains("collect::<Vec<_>>()"));
    assert!(body.contains("emit_model_state(&app, state.inner()).await"));
}

#[test]
fn model_switch_uses_consistent_settings_lock_order() {
    let source = include_str!("../src/commands.rs");
    let body = function_body(source, "pub async fn switch_model(", "// --- 认证 ---");

    let settings = body
        .find("let mut settings = state.runtime_settings.lock().await")
        .expect("runtime settings must be updated");
    let shared_model = body
        .find("*state.shared.model.lock().await")
        .expect("shared model must be updated");
    assert!(settings < shared_model);
}

#[test]
fn slash_model_switch_publishes_main_state() {
    let source = include_str!("../src/commands.rs");
    let body = function_body(
        source,
        "async fn switch_model_reference(",
        "fn parse_thinking_effort",
    );

    assert!(body.contains("emit_model_state(app, state).await"));
    assert!(source.contains("switch_model_reference(&state, &app, &args).await?"));
}
