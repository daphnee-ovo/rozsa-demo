// FrameworkTree
// first_send_session_test.rs
// └── first_send_materializes_before_dev_flow_refresh()

#[test]
fn first_send_materializes_before_dev_flow_refresh() {
    let source = include_str!("../src/commands.rs");
    let start = source
        .find("pub async fn send_message(")
        .expect("send_message command must exist");
    let end = source[start..]
        .find("pub struct SlashCommandResult")
        .expect("send_message boundary must exist");
    let body = &source[start..start + end];

    let append = body
        .find("append_custom(INTERACTION_STARTED.to_string(), None)")
        .expect("first interaction marker must be appended");
    let refresh = body
        .find("refresh_dev_flow_presentations(&state, idx).await?")
        .expect("Dev Flow presentations must be refreshed");
    assert!(
        append < refresh,
        "the lazy session must be materialized before opening it for Dev Flow refresh"
    );
}
