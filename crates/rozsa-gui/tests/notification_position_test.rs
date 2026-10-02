// FrameworkTree
// notification_position_test.rs
// ├── sidebar_feedback_forwards_to_the_global_center_and_preserves_delivery_errors()
// └── notifications_have_one_global_host_and_expand_down_from_the_top_right()

use std::process::Command;

#[test]
fn sidebar_feedback_forwards_to_the_global_center_and_preserves_delivery_errors() {
    let script = r#"
import assert from 'node:assert/strict';
import {createSidebarFeedback} from './crates/rozsa-gui/frontend/runtime/adapters/notifications.js';
const events=[];
const feedback=createSidebarFeedback({emit:async (...args)=>events.push(args)});
await feedback('Invalid session header');
assert.deepEqual(events,[['error','Invalid session header']]);
const failure=new Error('bridge unavailable');
await assert.rejects(createSidebarFeedback({emit:async()=>{throw failure;}})('failed'),error=>error===failure);
"#;
    let output = Command::new("node")
        .args(["--input-type=module", "-e", script])
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn notifications_have_one_global_host_and_expand_down_from_the_top_right() {
    let sidebar = include_str!("../frontend/sidebar.html");
    let entry = include_str!("../frontend/runtime/entries/sidebar.js");
    let css = include_str!("../frontend/ui/styles/composites/feedback.css");
    assert!(!sidebar.contains("data-template-mount=\"notification\""));
    assert!(!entry.contains("composites/notification.html"));
    let center = css
        .split(".notification-center {")
        .nth(1)
        .unwrap()
        .split('}')
        .next()
        .unwrap();
    assert!(center.contains("top: calc(var(--native-titlebar-offset) + 18px)"));
    assert!(center.contains("right: 20px"));
    let list = css
        .split(".notification-error-list {")
        .nth(1)
        .unwrap()
        .split('}')
        .next()
        .unwrap();
    assert!(list.contains("top: calc(100% + 8px)"));
    assert!(!list.contains("bottom:"));
}
