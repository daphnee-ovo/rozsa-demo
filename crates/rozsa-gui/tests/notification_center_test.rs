// FrameworkTree
// notification_center_test.rs
// ├── run_notification_behavior_harness()
// ├── upsert_event_serializes_as_tagged_camel_case()
// ├── resolve_event_serializes_with_id_only()
// ├── severities_serialize_lowercase()
// ├── notification_event_name_is_stable()
// ├── notification_state_behaves_under_updates_timers_and_user_interaction()
// ├── notification_layer_is_global_to_the_main_webview()
// ├── structured_and_legacy_notifications_are_both_supported()
// ├── stable_ids_deduplicate_toasts()
// ├── toasts_stack_downward_with_independent_six_second_timers()
// ├── hover_pauses_only_that_toasts_timer()
// ├── errors_collapse_into_unresolved_tray_with_circled_count()
// ├── closing_a_toast_does_not_resolve_the_condition()
// ├── error_list_supports_hover_focus_pin_and_escape()
// └── notification_accessibility_does_not_rely_on_color_alone()

use rozsa_app::model_registry::{ModelConfigDiagnostic, ModelConfigDiagnosticSeverity};
use rozsa_gui::notifications::{
    APP_NOTIFICATION_EVENT, AppNotificationEvent, NotificationSeverity, model_config_notification,
    reconcile_model_config_notifications,
};
use std::collections::HashSet;
use std::path::PathBuf;
use std::process::Command;

fn run_notification_behavior_harness(assertions: &str) {
    let source = include_str!("../frontend/app.js");
    let start = source
        .find("const notificationToasts = new Map();")
        .unwrap();
    let end = source[start..].find("function showHelp(topic)").unwrap() + start;
    let harness = r#"
class FakeElement {
  constructor(id = '') {
    this.id = id; this.children = []; this.parent = null; this.hidden = false;
    this.attributes = new Map(); this.listeners = {}; this.parts = new Map();
    this.className = ''; this._textContent = '';
  }
  set innerHTML(_value) { this.parts = new Map(); }
  set textContent(value) {
    this._textContent = String(value);
    if (value === '') this.children = [];
  }
  get textContent() { return this._textContent; }
  setAttribute(name, value) { this.attributes.set(name, String(value)); }
  getAttribute(name) { return this.attributes.get(name); }
  querySelector(selector) {
    if (!this.parts.has(selector)) this.parts.set(selector, new FakeElement(selector));
    return this.parts.get(selector);
  }
  addEventListener(name, callback) { this.listeners[name] = callback; }
  appendChild(child) { child.parent = this; this.children.push(child); return child; }
  remove() {
    if (!this.parent) return;
    this.parent.children = this.parent.children.filter(child => child !== this);
    this.parent = null;
  }
}
const elements = new Map([
  ['notificationStack', new FakeElement('notificationStack')],
  ['notificationErrorTray', new FakeElement('notificationErrorTray')],
  ['notificationErrorList', new FakeElement('notificationErrorList')],
  ['notificationErrorTrayButton', new FakeElement('notificationErrorTrayButton')],
  ['notificationErrorCount', new FakeElement('notificationErrorCount')],
]);
elements.get('notificationErrorList').hidden = true;
elements.get('notificationErrorTray').hidden = true;
const document = {
  getElementById: id => elements.get(id) || null,
  createElement: () => new FakeElement(),
};
let clock = 0;
let nextTimer = 1;
const timers = new Map();
const performance = { now: () => clock };
function setTimeout(callback, delay) {
  const id = nextTimer++;
  timers.set(id, { callback, at: clock + delay });
  return id;
}
function clearTimeout(id) { timers.delete(id); }
function advance(milliseconds) {
  const target = clock + milliseconds;
  while (true) {
    const due = Array.from(timers.entries())
      .filter(([, timer]) => timer.at <= target)
      .sort((left, right) => left[1].at - right[1].at)[0];
    if (!due) break;
    clock = due[1].at;
    timers.delete(due[0]);
    due[1].callback();
  }
  clock = target;
}
function check(condition, message) { if (!condition) throw new Error(message); }
"#;
    let mut script = String::with_capacity(harness.len() + end - start + assertions.len());
    script.push_str(harness);
    script.push_str(&source[start..end]);
    script.push_str(assertions);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("notification_behavior_test.js");
    std::fs::write(&path, script).unwrap();
    let output = Command::new("node")
        .arg(&path)
        .output()
        .expect("Node.js is required for frontend behavior tests");
    assert!(
        output.status.success(),
        "notification behavior harness failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn upsert_event_serializes_as_tagged_camel_case() {
    let event = AppNotificationEvent::Upsert {
        id: "dev-flow.connection:abc".to_owned(),
        severity: NotificationSeverity::Error,
        title: "Connection lost".to_owned(),
        message: "SSE stalled".to_owned(),
        timeout_ms: 6000,
    };
    let value = serde_json::to_value(&event).unwrap();
    assert_eq!(value["type"], "upsert");
    assert_eq!(value["id"], "dev-flow.connection:abc");
    assert_eq!(value["severity"], "error");
    assert_eq!(value["title"], "Connection lost");
    assert_eq!(value["message"], "SSE stalled");
    assert_eq!(value["timeoutMs"], 6000);
}

#[test]
fn resolve_event_serializes_with_id_only() {
    let value = serde_json::to_value(AppNotificationEvent::Resolve {
        id: "dev-flow.cli".to_owned(),
    })
    .unwrap();
    assert_eq!(value["type"], "resolve");
    assert_eq!(value["id"], "dev-flow.cli");
}

#[test]
fn severities_serialize_lowercase() {
    for (severity, expected) in [
        (NotificationSeverity::Info, "info"),
        (NotificationSeverity::Success, "success"),
        (NotificationSeverity::Warning, "warning"),
        (NotificationSeverity::Error, "error"),
    ] {
        assert_eq!(
            serde_json::to_value(severity).unwrap(),
            serde_json::json!(expected)
        );
    }
}

#[test]
fn notification_event_name_is_stable() {
    assert_eq!(APP_NOTIFICATION_EVENT, "app-notification");
}

#[test]
fn notification_state_behaves_under_updates_timers_and_user_interaction() {
    run_notification_behavior_harness(
        r#"
upsertNotification({ id: 'changing', severity: 'error', title: 'First', message: 'old', timeoutMs: 6000 });
upsertNotification({ id: 'changing', severity: 'warning', title: 'Second', message: 'new', timeoutMs: 6000 });
let changing = notificationToasts.get('changing');
check(notificationToasts.size === 1, 'same ID must update rather than duplicate');
check(changing.severity === 'warning', 'stored severity must update');
check(changing.element.className.endsWith('notification-warning'), 'severity class must update');
check(changing.element.getAttribute('role') === 'status', 'ARIA role must update');
check(changing.element.querySelector('.notification-icon').textContent === 'i', 'icon must update');
advance(5999);
check(notificationToasts.has('changing'), 'updated timer must run independently');
advance(1);
check(!notificationToasts.has('changing') && unresolvedErrors.size === 0, 'non-error expiry must resolve visually');

upsertNotification({ id: 'first', severity: 'error', title: 'First error', message: 'a', timeoutMs: 6000 });
advance(1000);
upsertNotification({ id: 'second', severity: 'error', title: 'Second error', message: 'b', timeoutMs: 6000 });
const second = notificationToasts.get('second');
second.element.listeners.pointerenter();
upsertNotification({ id: 'second', severity: 'error', title: 'Second error updated', message: 'updated while hovered', timeoutMs: 6000 });
advance(5000);
check(!notificationToasts.has('first'), 'first independent timer must expire');
check(notificationToasts.has('second'), 'same-ID update must preserve the hovered pause');
advance(1000);
check(notificationToasts.has('second'), 'hovered update must not silently restart its timer');
check(elements.get('notificationStack').children[0] === second.element, 'remaining toast must reflow upward');
second.element.listeners.pointerleave();
advance(5999);
check(notificationToasts.has('second'), 'resumed toast must retain its independent updated timeout');
advance(1);
check(!notificationToasts.has('second'), 'paused toast must expire after its own remaining time');
check(unresolvedErrors.size === 2, 'expired errors remain unresolved');

upsertNotification({ id: 'first', severity: 'error', title: 'First updated', message: 'changed', timeoutMs: 6000 });
check(notificationToasts.size === 0, 'unresolved error update must not recreate a toast');
check(unresolvedErrors.size === 2, 'unresolved update must not increase count');
check(unresolvedErrors.get('first').message === 'changed', 'unresolved content must update');
resolveNotification('first');
check(unresolvedErrors.size === 1, 'Resolve must reduce unresolved count');

upsertNotification({ id: 'dismissed', severity: 'error', title: 'Dismissed', message: 'still broken', timeoutMs: 6000 });
dismissToast('dismissed');
check(unresolvedErrors.has('dismissed'), 'dismiss must not resolve an error');

setupNotificationErrorTray();
const tray = elements.get('notificationErrorTray');
const button = elements.get('notificationErrorTrayButton');
tray.listeners.pointerenter();
check(!elements.get('notificationErrorList').hidden, 'tray hover must expand the list');
tray.listeners.pointerleave();
check(elements.get('notificationErrorList').hidden, 'un-pinned hover list must close');
button.listeners.click();
tray.listeners.pointerleave();
check(!elements.get('notificationErrorList').hidden, 'click must pin the list');
closeNotificationErrorList();
check(elements.get('notificationErrorList').hidden, 'Escape close path must close and unpin');
"#,
    );
}

#[test]
fn notification_layer_is_global_to_the_main_webview() {
    let html = include_str!("../frontend/index.html");
    let css = include_str!("../frontend/styles/components/feedback.css");
    let main_start = html.find("id=\"mainContentScene\"").unwrap();
    let settings_start = html.find("id=\"settingsPanel\"").unwrap();
    let center = html.find("id=\"notificationCenter\"").unwrap();
    assert!(
        center > main_start,
        "notification layer must live outside the Main root"
    );
    assert!(
        center > settings_start,
        "notification layer must live outside the Settings root"
    );
    assert!(html.contains("class=\"notification-center\""));
    assert!(html.contains("aria-live=\"polite\""));
    assert!(css.contains("position: fixed;"));
    assert!(css.contains("top: 12px;"));
    assert!(css.contains("right: 12px;"));
    assert!(css.contains("z-index: 200;"));
}

#[test]
fn structured_and_legacy_notifications_are_both_supported() {
    let js = include_str!("../frontend/app.js");
    assert!(js.contains("await listen('app-notification'"));
    assert!(js.contains("payload.type === 'upsert'"));
    assert!(js.contains("payload.type === 'resolve'"));
    assert!(js.contains("function upsertNotification(payload)"));
    assert!(js.contains("function resolveNotification(id)"));
    assert!(js.contains("await listen('notification'"));
    assert!(js.contains("function showNotification(message)"));
    assert!(js.contains("'legacy-' + legacyNotificationCounter"));
    assert!(js.contains("severity: 'info'"));
    assert!(js.contains("timeoutMs: NOTIFICATION_TIMEOUT_MS"));
}

#[test]
fn stable_ids_deduplicate_toasts() {
    let js = include_str!("../frontend/app.js");
    assert!(js.contains("const notificationToasts = new Map();"));
    let dedup = js
        .find("const existing = notificationToasts.get(id);")
        .unwrap();
    let update = js
        .find("applyNotificationPresentation(existing, severity, title, message);")
        .unwrap();
    assert!(
        dedup < update,
        "existing toast is updated in place instead of duplicated"
    );
    assert!(js.contains("if (!existing.paused)"));
}

#[test]
fn toasts_stack_downward_with_independent_six_second_timers() {
    let js = include_str!("../frontend/app.js");
    let css = include_str!("../frontend/styles/components/feedback.css");
    assert!(js.contains("const NOTIFICATION_TIMEOUT_MS = 6000;"));
    assert!(js.contains("notificationStack().appendChild(element);"));
    assert!(js.contains("toast.timer = setTimeout(() => expireNotification(id), timeoutMs);"));
    assert!(css.contains("flex-direction: column;"));
    assert!(js.contains("toast.element.remove();"));
}

#[test]
fn hover_pauses_only_that_toasts_timer() {
    let js = include_str!("../frontend/app.js");
    assert!(js.contains("element.addEventListener('pointerenter', () => pauseToastTimer(id));"));
    assert!(js.contains("element.addEventListener('pointerleave', () => resumeToastTimer(id));"));
    assert!(js.contains("function pauseToastTimer(id)"));
    assert!(js.contains("clearTimeout(toast.timer);"));
    assert!(js.contains("toast.remainingMs = Math.max(0, toast.expiresAt - performance.now());"));
    assert!(js.contains("function resumeToastTimer(id)"));
    assert!(js.contains("toast.paused = true;"));
    assert!(js.contains("toast.paused = false;"));
    assert!(js.contains("toast.expiresAt = performance.now() + toast.remainingMs;"));
}

#[test]
fn errors_collapse_into_unresolved_tray_with_circled_count() {
    let js = include_str!("../frontend/app.js");
    let html = include_str!("../frontend/index.html");
    let css = include_str!("../frontend/styles/components/feedback.css");
    assert!(js.contains("if (toast.severity === 'error') {"));
    assert!(
        js.contains("unresolvedErrors.set(id, { title: toast.title, message: toast.message });")
    );
    assert!(js.contains("function updateNotificationErrorTray()"));
    assert!(js.contains("notificationErrorCount().textContent = String(count);"));
    assert!(js.contains("notificationErrorTray().hidden = count === 0;"));
    assert!(html.contains("id=\"notificationErrorTrayButton\""));
    assert!(html.contains("id=\"notificationErrorCount\""));
    assert!(html.contains("class=\"notification-error-icon\""));
    assert!(css.contains("border-radius: 50%;"));
}

#[test]
fn closing_a_toast_does_not_resolve_the_condition() {
    let js = include_str!("../frontend/app.js");
    let dismiss = js.find("function dismissToast(id)").unwrap();
    let resolve = js.find("function resolveNotification(id)").unwrap();
    let dismiss_body = &js[dismiss..resolve];
    assert!(
        dismiss_body.contains("unresolvedErrors.set(id,"),
        "dismissing an error toast keeps the condition unresolved"
    );
    assert!(
        !dismiss_body.contains("unresolvedErrors.delete(id)"),
        "only Resolve may reduce the unresolved count"
    );
    let resolve_body = &js[resolve..];
    assert!(resolve_body.contains("unresolvedErrors.delete(id);"));
    assert!(js.contains("'click', () => dismissToast(id)"));
}

#[test]
fn error_list_supports_hover_focus_pin_and_escape() {
    let js = include_str!("../frontend/app.js");
    let html = include_str!("../frontend/index.html");
    assert!(js.contains("tray.addEventListener('pointerenter'"));
    assert!(js.contains("tray.addEventListener('pointerleave'"));
    assert!(js.contains("if (!notificationErrorListPinned) closeNotificationErrorList();"));
    assert!(js.contains("button.addEventListener('click'"));
    assert!(js.contains("notificationErrorListPinned = true;"));
    assert!(js.contains("button.addEventListener('focusin'"));
    assert!(js.contains("button.addEventListener('focusout'"));
    assert!(js.contains("function openNotificationErrorList()"));
    assert!(js.contains("function closeNotificationErrorList()"));
    assert!(html.contains("aria-expanded=\"false\""));
    assert!(html.contains("aria-label=\"Unresolved errors\""));
    assert!(html.contains("role=\"list\""));
    let keydown_start = js.find("document.addEventListener('keydown'").unwrap();
    let keydown = &js[keydown_start..];
    let notification_escape = keydown.find("!notificationErrorList().hidden").unwrap();
    let double_escape = keydown.find("if (isDoubleEscape)").unwrap();
    assert!(
        notification_escape < double_escape,
        "Escape closes the notification list before streaming escape handling"
    );
    assert!(keydown.contains("closeNotificationErrorList();"));
}

#[test]
fn notification_accessibility_does_not_rely_on_color_alone() {
    let js = include_str!("../frontend/app.js");
    let html = include_str!("../frontend/index.html");
    let css = include_str!("../frontend/styles/components/feedback.css");
    assert!(html.contains("aria-hidden=\"true\""));
    assert!(js.contains("notificationIcon(severity)"));
    assert!(js.contains("setAttribute('role', severity === 'error' ? 'alert' : 'status')"));
    assert!(css.contains(".notification-error { border-color: var(--error); }"));
    assert!(js.contains("aria-label=\"Dismiss notification\""));
}

#[test]
fn model_config_diagnostics_map_to_shared_notifications_and_resolve() {
    let error = ModelConfigDiagnostic {
        path: PathBuf::from("/tmp/broken.json"),
        severity: ModelConfigDiagnosticSeverity::Error,
        provider: None,
        reference: None,
        message: "Failed to parse models.json".to_string(),
        hint: "Fix the JSON/schema error".to_string(),
    };
    let warning = ModelConfigDiagnostic {
        path: PathBuf::from("/Users/test/.rozsa/models/deepseek.json"),
        severity: ModelConfigDiagnosticSeverity::Warning,
        provider: Some("deepseek".to_string()),
        reference: Some("$DEEPSEEK_API_KEY".to_string()),
        message: "environment variable could not be resolved".to_string(),
        hint: "Check the variable name".to_string(),
    };

    assert_eq!(
        model_config_notification(&error),
        AppNotificationEvent::Upsert {
            id: error.notification_id(),
            severity: NotificationSeverity::Error,
            title: "Model configuration error".to_string(),
            message:
                "Failed to parse models.json File: /tmp/broken.json Hint: Fix the JSON/schema error"
                    .to_string(),
            timeout_ms: 6_000,
        }
    );

    let mut active_ids = HashSet::new();
    active_ids.insert(error.notification_id());
    let (events, next_ids) = reconcile_model_config_notifications(&active_ids, &[warning.clone()]);
    assert!(events.contains(&AppNotificationEvent::Resolve {
        id: error.notification_id(),
    }));
    assert!(events.contains(&model_config_notification(&warning)));
    assert!(!next_ids.contains(&error.notification_id()));
    assert!(next_ids.contains(&warning.notification_id()));
}
