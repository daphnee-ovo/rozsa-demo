// Native component ownership remains independent of the migration prototype.
#[test]
fn shared_state_rules_live_with_component_owners() {
    let composer = include_str!("../frontend/ui/styles/composites/composer.css");
    let queue = include_str!("../frontend/ui/styles/features/queue.css");
    let steering = include_str!("../frontend/ui/styles/features/steering.css");
    let sidebar = include_str!("../frontend/ui/styles/features/sidebar.css");
    let dev_flow = include_str!("../frontend/ui/styles/features/dev-flow-runtime.css");
    let overlays = include_str!("../frontend/ui/styles/composites/permission-question.css");
    assert!(composer.contains(".input-wrapper > .running-messages:not([hidden]) ~ .rich-input"));
    assert!(composer.contains("[data-od-id=\"chat-input\"] { position: absolute;"));
    assert!(queue.contains("#queuedMessages"));
    assert!(steering.contains("#steeringConversation"));
    assert!(sidebar.contains(".subagent-sidebar-panel"));
    assert!(dev_flow.contains(".dev-flow-detail"));
    assert!(overlays.contains(":has(> .question-panel.visible)"));
}
