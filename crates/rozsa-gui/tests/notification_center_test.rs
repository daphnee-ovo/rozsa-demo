// FrameworkTree
// notification_center_test.rs
// ├── run_notification_behavior_harness()
// ├── run_notification_behavior_harness_with_component()
// ├── run_notification_markup_harness()
// ├── upsert_event_serializes_as_tagged_camel_case()
// ├── resolve_event_serializes_with_id_only()
// ├── severities_serialize_lowercase()
// ├── notification_event_name_is_stable()
// ├── notification_state_behaves_under_updates_timers_and_user_interaction()
// ├── notification_layer_is_global_to_the_main_webview()
// ├── shared_notification_markup_escapes_untrusted_content()
// ├── structured_and_legacy_notifications_are_both_supported()
// ├── stable_ids_deduplicate_toasts()
// ├── toasts_stack_downward_with_independent_six_second_timers()
// ├── hover_pauses_only_that_toasts_timer()
// ├── errors_collapse_into_unresolved_tray_with_circled_count()
// ├── focus_notification_tray_then_click_pins_instead_of_closing()
// ├── closing_a_toast_does_not_resolve_the_condition()
// ├── error_list_supports_hover_focus_pin_and_escape()
// ├── notification_accessibility_does_not_rely_on_color_alone()
// └── model_config_diagnostics_map_to_shared_notifications_and_resolve()

use rozsa_app::model_registry::{ModelConfigDiagnostic, ModelConfigDiagnosticSeverity};
use rozsa_gui::notifications::{
    APP_NOTIFICATION_EVENT, AppNotificationEvent, NotificationSeverity, model_config_notification,
    reconcile_model_config_notifications,
};
use std::collections::HashSet;
use std::path::PathBuf;
use std::process::Command;

fn run_notification_behavior_harness(assertions: &str) {
    run_notification_behavior_harness_with_component(
        assertions,
        include_str!("../frontend/ui/scripts/components/notification.js"),
    )
    .unwrap_or_else(|error| panic!("notification behavior harness failed: {error}"));
}

fn run_notification_behavior_harness_with_component(
    assertions: &str,
    shared_component: &str,
) -> Result<(), String> {
    let source = include_str!("../frontend/app.js");
    let shared_features = include_str!("../frontend/ui/scripts/features/notifications.js");
    let start = source
        .find("const notificationToasts = new Map();")
        .unwrap();
    let end = source[start..].find("function showHelp(topic)").unwrap() + start;
    let harness = r#"
class FakeElement {
  constructor(id = '') {
    this.id = id; this.children = []; this.parent = null; this.hidden = false;
    this.attributes = new Map(); this.listeners = {}; this.parts = new Map();
    this.className = ''; this._textContent = ''; this._innerHTML = '';
  }
  set innerHTML(value) { this._innerHTML = String(value); this.parts = new Map(); }
  get innerHTML() { return this._innerHTML; }
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
    let mut script = String::with_capacity(
        harness.len() + shared_features.len() + shared_component.len() + end - start
            + assertions.len(),
    );
    script.push_str(harness);
    script.push_str("const notificationModule = (() => {\n");
    script.push_str(&shared_features.replace("export ", ""));
    script.push_str(&shared_component.replace("export ", ""));
    script.push_str(
        r#"
return { notificationIcon, notificationMarkup, errorItemMarkup, applyNotificationPresentation,
  createNotificationElement, bindToast, bindErrorTray, init };
})();
"#,
    );
    script.push_str(r#"
const window = { RozsaNotifications: {
  ...notificationModule,
  createNotificationElement(item) {
    const element = new FakeElement();
    element.dataset = { notificationId: item.id };
    for (const selector of ['.notification-icon', '.notification-title', '.notification-message', '.notification-close']) {
      element.parts.set(selector, new FakeElement(selector));
    }
    return element;
  },
}};
"#);
    script.push_str(&source[start..end]);
    script.push_str(assertions);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("notification_behavior_test.js");
    std::fs::write(&path, script).unwrap();
    let output = Command::new("node")
        .arg(&path)
        .output()
        .expect("Node.js is required for frontend behavior tests");
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).into_owned())
    }
}

fn run_notification_markup_harness(assertions: &str) {
    let features = include_str!("../frontend/ui/scripts/features/notifications.js");
    let component = include_str!("../frontend/ui/scripts/components/notification.js");
    let adapter = include_str!("../frontend/runtime/adapters/notifications.js");
    let harness = r#"
function check(condition, message) { if (!condition) throw new Error(message); }
"#;
    let mut script = String::with_capacity(
        features.len() + component.len() + adapter.len() + harness.len() + assertions.len() + 512,
    );
    script.push_str(harness);
    script.push_str(&features.replace("export ", ""));
    script.push_str(&component.replace("export ", ""));
    script.push_str(
        &adapter
            .lines()
            .filter(|line| !line.starts_with("import "))
            .collect::<Vec<_>>()
            .join("\n"),
    );
    script.push_str(assertions);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("notification_markup_test.js");
    std::fs::write(&path, script).unwrap();
    let output = Command::new("node")
        .arg(&path)
        .output()
        .expect("Node.js is required for frontend markup tests");
    assert!(
        output.status.success(),
        "notification markup harness failed: {}",
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
check(changing.element.querySelector('.notification-icon').innerHTML.includes('data-lucide="triangle-alert"'), 'shared severity icon must update');
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
button.listeners.focusin();
button.listeners.focusout();
check(!elements.get('notificationErrorList').hidden, 'keyboard focus must keep a pinned list open');
closeNotificationErrorList();
check(elements.get('notificationErrorList').hidden, 'Escape close path must close and unpin');
"#,
    );
}

#[test]
fn notification_layer_is_global_to_the_main_webview() {
    let html = include_str!("../frontend/index.html");
    let template = include_str!("../frontend/ui/templates/composites/notification.html");
    let entry = include_str!("../frontend/runtime/entries/main.js");
    let main_start = html.find("id=\"mainContentScene\"").unwrap();
    let settings_start = html.find("id=\"settingsPanel\"").unwrap();
    let center = html.find("data-template-mount=\"notification\"").unwrap();
    assert!(
        center > main_start,
        "notification layer must live outside the Main root"
    );
    assert!(
        center > settings_start,
        "notification layer must live outside the Settings root"
    );
    assert!(template.contains("id=\"notificationCenter\""));
    assert!(template.contains("class=\"notification-center\""));
    assert!(template.contains("aria-live=\"polite\""));
    assert!(template.contains("id=\"notificationStack\""));
    assert!(template.contains("id=\"notificationErrorTray\""));
    assert!(entry.contains("composites/notification.html"));
    assert!(entry.contains("selector: \"[data-template-mount=notification]\""));
}

#[test]
fn shared_notification_markup_escapes_untrusted_content() {
    run_notification_markup_harness(
        r#"
const input = { id: `id<&>'\"`, severity: `error<&>'\"`, title: `<img src=x onerror=alert(1)> "title"`, message: `A & B <script>alert('x')</script>` };
const markup = notificationMarkup(input);
check(markup.includes('data-notification-id="id&lt;&amp;&gt;&#39;&quot;"'), 'notification id must be escaped in the data attribute');
check(markup.includes('notification-error&lt;&amp;&gt;&#39;&quot;'), 'severity must be escaped in the class');
check(markup.includes('&lt;img src=x onerror=alert(1)&gt; &quot;title&quot;'), 'notification title must be escaped');
check(markup.includes('A &amp; B &lt;script&gt;alert(&#39;x&#39;)&lt;/script&gt;'), 'notification message must be escaped');
check(!markup.includes('<script>') && !markup.includes('<img'), 'untrusted text must not become markup');

const error = errorItemMarkup({ title: `<svg onload=alert(1)>`, message: `bad & <broken>` });
check(error.includes('&lt;svg onload=alert(1)&gt;'), 'error title must be escaped');
check(error.includes('bad &amp; &lt;broken&gt;'), 'error message must be escaped');
check(error.includes('role="listitem"'), 'shared error entry keeps list semantics');
"#,
    );
}

#[test]
fn structured_and_legacy_notifications_are_both_supported() {
    run_notification_markup_harness(
        r#"
;(async () => {
  const handlers = new Map();
  const port = { subscribe: async (event, handler) => { handlers.set(event, handler); } };
  const upserts = [];
  const resolved = [];
  const legacy = [];
  const errors = [];
  await subscribeNotifications(port, {
    upsertNotification: event => upserts.push(event),
    resolveNotification: id => resolved.push(id),
    showNotification: message => legacy.push(message),
    showError: message => errors.push(message),
  });
  await handlers.get('app-notification')({ type: 'upsert', id: 'backend:stable', severity: 'error', title: 'Failure', message: 'broken', timeoutMs: 3000 });
  check(upserts.length === 1 && upserts[0].id === 'backend:stable', 'valid backend upsert must preserve its stable identity');
  await handlers.get('app-notification')({ type: 'resolve', id: 'backend:stable' });
  check(resolved.length === 1 && resolved[0] === 'backend:stable', 'valid backend resolve must preserve its identity');
  await handlers.get('app-notification')({ type: 'upsert', id: 'invalid', severity: 'fatal', title: 'Failure', message: 'bad severity' });
  check(upserts.length === 1 && errors.length === 1 && errors[0].includes('Invalid app-notification event'), 'invalid backend severity must report an adapter error and emit no upsert');
  await handlers.get('app-notification')({ type: 'resolve', id: '' });
  check(resolved.length === 1 && errors.length === 2, 'empty resolve identity must report an adapter error');
  await handlers.get('notification')('legacy message');
  check(legacy.length === 1 && legacy[0] === 'legacy message', 'legacy notification event must remain supported');
})().catch(error => { setTimeout(() => { throw error; }, 0); });
"#,
    );
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
    let css = include_str!("../frontend/ui/styles/composites/feedback.css");
    assert!(js.contains("const NOTIFICATION_TIMEOUT_MS = 6000;"));
    assert!(js.contains("notificationStack().appendChild(element);"));
    assert!(js.contains("toast.timer = setTimeout(() => expireNotification(id), timeoutMs);"));
    assert!(css.contains("display: grid; gap: 8px;"));
    assert!(js.contains("toast.element.remove();"));
}

#[test]
fn hover_pauses_only_that_toasts_timer() {
    let component = include_str!("../frontend/ui/scripts/components/notification.js");
    assert!(component.contains("export function bindToast(element, id, api)"));
    assert!(component.contains("api.pauseToastTimer?.(id)"));
    assert!(component.contains("api.resumeToastTimer?.(id)"));
    run_notification_behavior_harness(
        r#"
upsertNotification({ id: 'hover', severity: 'info', title: 'Hover', message: 'pause', timeoutMs: 6000 });
const hover = notificationToasts.get('hover');
advance(2000);
hover.element.listeners.pointerenter();
advance(10000);
check(notificationToasts.has('hover'), 'hover pauses only the hovered toast timer');
hover.element.listeners.pointerleave();
advance(3999);
check(notificationToasts.has('hover'), 'resumed toast keeps its remaining duration');
advance(1);
check(!notificationToasts.has('hover'), 'resumed toast expires after the remaining duration');
"#,
    );
}

#[test]
fn errors_collapse_into_unresolved_tray_with_circled_count() {
    let js = include_str!("../frontend/app.js");
    let template = include_str!("../frontend/ui/templates/composites/notification.html");
    assert!(js.contains("function updateNotificationErrorTray()"));
    assert!(js.contains("notificationErrorCount().textContent = String(count);"));
    assert!(js.contains("notificationErrorTray().hidden = count === 0;"));
    assert!(template.contains("id=\"notificationErrorTrayButton\""));
    assert!(template.contains("id=\"notificationErrorCount\""));
    assert!(template.contains("class=\"notification-error-icon\""));
}

#[test]
fn focus_notification_tray_then_click_pins_instead_of_closing() {
    let assertions = r#"
upsertNotification({ id: 'focused-tray', severity: 'error', title: 'Focused', message: 'active', timeoutMs: 6000 });
dismissToast('focused-tray');
setupNotificationErrorTray();
const tray = elements.get('notificationErrorTray');
const button = elements.get('notificationErrorTrayButton');
const list = elements.get('notificationErrorList');
button.listeners.focusin();
check(!list.hidden, 'focus opens the unresolved error list');
button.listeners.click();
check(!list.hidden, 'click after focus pins the already-open list instead of closing it');
button.listeners.focusout();
tray.listeners.pointerleave();
check(!list.hidden, 'pinned list stays open after focus and pointer leave');
button.listeners.click();
check(list.hidden, 'a second click closes a pinned list');
"#;
    let component = include_str!("../frontend/ui/scripts/components/notification.js");
    let previous_click_condition = component.replace(
        "if (pinned) close();\n    else { api.openNotificationErrorList(); pinned = true; }",
        "if (api.isNotificationErrorListOpen()) close();\n    else { api.openNotificationErrorList(); pinned = true; }",
    );
    assert_ne!(
        previous_click_condition, component,
        "test fixture must model the pre-fix click condition"
    );
    let previous_result =
        run_notification_behavior_harness_with_component(assertions, &previous_click_condition);
    assert!(
        previous_result
            .expect_err("pre-fix focus-then-click behavior should fail")
            .contains("click after focus pins the already-open list instead of closing it"),
        "the regression scenario must fail under the previous behavior"
    );
    run_notification_behavior_harness(assertions);
}

#[test]
fn closing_a_toast_does_not_resolve_the_condition() {
    run_notification_behavior_harness(
        r#"
upsertNotification({ id: 'dismiss', severity: 'error', title: 'Dismiss', message: 'still failing', timeoutMs: 6000 });
elements.get('notificationStack').children[0].querySelector('.notification-close').listeners.click();
check(!notificationToasts.has('dismiss'), 'closing removes the visual toast');
check(unresolvedErrors.has('dismiss'), 'closing keeps the condition unresolved');
resolveNotification('dismiss');
check(!unresolvedErrors.has('dismiss'), 'only a resolve event clears the unresolved condition');
"#,
    );
}

#[test]
fn error_list_supports_hover_focus_pin_and_escape() {
    run_notification_behavior_harness(
        r#"
upsertNotification({ id: 'tray', severity: 'error', title: 'Tray', message: 'active', timeoutMs: 6000 });
dismissToast('tray');
setupNotificationErrorTray();
const tray = elements.get('notificationErrorTray');
const button = elements.get('notificationErrorTrayButton');
const list = elements.get('notificationErrorList');
tray.listeners.pointerenter();
check(!list.hidden, 'hover opens the unresolved error list');
tray.listeners.pointerleave();
check(list.hidden, 'un-pinned hover leave closes the list');
button.listeners.focusin();
check(!list.hidden, 'keyboard focus opens the list');
button.listeners.focusout();
check(list.hidden, 'un-pinned focus leave closes the list');
button.listeners.click();
check(!list.hidden, 'click pins the list open');
tray.listeners.pointerleave();
button.listeners.focusout();
check(!list.hidden, 'pinned list stays open across pointer and focus leave');
closeNotificationErrorList();
check(list.hidden && button.getAttribute('aria-expanded') === 'false', 'Escape close path hides and unpins the list');
"#,
    );
    let js = include_str!("../frontend/app.js");
    let keydown_start = js
        .find("document.addEventListener('keydown', function(e)")
        .unwrap();
    let keydown = &js[keydown_start..];
    let notification_escape = keydown.find("!notificationErrorList().hidden").unwrap();
    let streaming_escape = keydown.find("if (isDoubleEscape)").unwrap();
    assert!(notification_escape < streaming_escape);
    assert!(keydown.contains("closeNotificationErrorList();"));
}

#[test]
fn notification_accessibility_does_not_rely_on_color_alone() {
    run_notification_markup_harness(
        r#"
const info = notificationMarkup({ id: 'info', severity: 'info', title: 'Info', message: 'ready' });
const error = notificationMarkup({ id: 'error', severity: 'error', title: 'Error', message: 'failed' });
check(info.includes('role="status"'), 'non-error notification has status semantics');
check(error.includes('role="alert"'), 'error notification has alert semantics');
check(info.includes('aria-hidden="true"') && error.includes('aria-label="Dismiss notification"'), 'icon and dismiss control have accessible labels');
check(notificationIcon('error').includes('data-lucide="circle-x"'), 'error severity has a distinct semantic icon');
"#,
    );
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
