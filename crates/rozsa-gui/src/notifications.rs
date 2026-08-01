// FrameworkTree
// notifications.rs
// ├── enum NotificationSeverity
// ├── enum AppNotificationEvent
// ├── model_config_notification()
// ├── reconcile_model_config_notifications()
// └── emit_notification()

//! Reusable in-app notification events for the main WebView.
//!
//! The notification layer is a main-WebView component outside the Main and
//! Settings scene roots. Backend code emits structured `AppNotificationEvent`
//! payloads; legacy string `notification` events remain supported and are
//! mapped to nonpersistent info toasts by the frontend.

use serde::Serialize;
use tauri::{AppHandle, Runtime};

use rozsa_app::model_registry::{ModelConfigDiagnostic, ModelConfigDiagnosticSeverity};

use crate::events::emit_main;

pub const APP_NOTIFICATION_EVENT: &str = "app-notification";

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum NotificationSeverity {
    Info,
    Success,
    Warning,
    Error,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum AppNotificationEvent {
    #[serde(rename_all = "camelCase")]
    Upsert {
        id: String,
        severity: NotificationSeverity,
        title: String,
        message: String,
        timeout_ms: u64,
    },
    Resolve {
        id: String,
    },
}

/// Convert a model registry diagnostic into the shared application
/// notification event used by the main WebView.
pub fn model_config_notification(diagnostic: &ModelConfigDiagnostic) -> AppNotificationEvent {
    let severity = match diagnostic.severity {
        ModelConfigDiagnosticSeverity::Error => NotificationSeverity::Error,
        ModelConfigDiagnosticSeverity::Warning => NotificationSeverity::Warning,
    };
    let title = match diagnostic.severity {
        ModelConfigDiagnosticSeverity::Error => "Model configuration error",
        ModelConfigDiagnosticSeverity::Warning => "Model environment warning",
    };
    let message = format!(
        "{} File: {} Hint: {}",
        diagnostic.message,
        diagnostic.path.display(),
        diagnostic.hint
    );
    AppNotificationEvent::Upsert {
        id: diagnostic.notification_id(),
        severity,
        title: title.to_string(),
        message,
        timeout_ms: 6_000,
    }
}

/// Reconcile model configuration notification IDs after a registry reload.
/// Resolved diagnostics are removed from the existing error tray through the
/// same notification channel used for their original upserts.
pub fn reconcile_model_config_notifications(
    active_ids: &std::collections::HashSet<String>,
    diagnostics: &[ModelConfigDiagnostic],
) -> (Vec<AppNotificationEvent>, std::collections::HashSet<String>) {
    let current_ids = diagnostics
        .iter()
        .map(ModelConfigDiagnostic::notification_id)
        .collect::<std::collections::HashSet<_>>();
    let mut events = active_ids
        .difference(&current_ids)
        .map(|id| AppNotificationEvent::Resolve { id: id.clone() })
        .collect::<Vec<_>>();
    events.extend(diagnostics.iter().map(model_config_notification));
    (events, current_ids)
}

pub fn emit_notification<R: Runtime>(
    app: &AppHandle<R>,
    event: AppNotificationEvent,
) -> Result<(), String> {
    emit_main(app, APP_NOTIFICATION_EVENT, event)
}
