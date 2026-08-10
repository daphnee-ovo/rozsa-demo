// FrameworkTree
// merge.rs
// ├── merge_settings()
// ├── merge_dev_flow()
// ├── merge_capabilities()
// ├── merge_appearance()
// ├── merge_compaction()
// ├── merge_retry()
// └── merge_permissions()

use super::schema::{
    AppearanceSettings, CompactionSettings, DevFlowSettings, PartialAppearanceSettings,
    PartialCompactionSettings, PartialDevFlowSettings, PartialPermissionSettings,
    PartialRetrySettings, PartialSettings, PermissionSettings, RetrySettings, Settings,
};

/// Merge settings: base + overlay → resolved
/// For each field: if overlay has Some, use it; else keep base
pub fn merge_settings(base: &Settings, overlay: &PartialSettings) -> Settings {
    Settings {
        default_provider: overlay
            .default_provider
            .clone()
            .or_else(|| base.default_provider.clone()),
        default_model: overlay
            .default_model
            .clone()
            .or_else(|| base.default_model.clone()),
        small_model: overlay
            .small_model
            .clone()
            .or_else(|| base.small_model.clone()),
        default_thinking_effort: overlay
            .default_thinking_effort
            .or(base.default_thinking_effort),
        compaction: merge_compaction(&base.compaction, overlay.compaction.as_ref()),
        retry: merge_retry(&base.retry, overlay.retry.as_ref()),
        transport: overlay
            .transport
            .clone()
            .unwrap_or_else(|| base.transport.clone()),
        block_images: overlay.block_images.unwrap_or(base.block_images),
        hide_thinking: overlay.hide_thinking.unwrap_or(base.hide_thinking),
        auto_session_naming: overlay
            .auto_session_naming
            .unwrap_or(base.auto_session_naming),
        steering_mode: overlay
            .steering_mode
            .clone()
            .unwrap_or_else(|| base.steering_mode.clone()),
        follow_up_mode: overlay
            .follow_up_mode
            .clone()
            .unwrap_or_else(|| base.follow_up_mode.clone()),
        running_send_mode: overlay
            .running_send_mode
            .clone()
            .unwrap_or_else(|| base.running_send_mode.clone()),
        permissions: merge_permissions(&base.permissions, overlay.permissions.as_ref()),
        context_window_preferences: overlay
            .context_window_preferences
            .clone()
            .unwrap_or_else(|| base.context_window_preferences.clone()),
        lsp_mode: overlay
            .lsp_mode
            .clone()
            .unwrap_or_else(|| base.lsp_mode.clone()),
        tools: merge_capabilities(&base.tools, overlay.tools.as_ref()),
        skills: merge_capabilities(&base.skills, overlay.skills.as_ref()),
        appearance: merge_appearance(&base.appearance, overlay.appearance.as_ref()),
        dev_flow: merge_dev_flow(&base.dev_flow, overlay.dev_flow.as_ref()),
    }
}

fn merge_dev_flow(
    base: &DevFlowSettings,
    overlay: Option<&PartialDevFlowSettings>,
) -> DevFlowSettings {
    match overlay {
        None => base.clone(),
        Some(overlay) => DevFlowSettings {
            enabled: overlay.enabled.unwrap_or(base.enabled),
            show_sidebar_status: overlay
                .show_sidebar_status
                .unwrap_or(base.show_sidebar_status),
            show_dashboard_button: overlay
                .show_dashboard_button
                .unwrap_or(base.show_dashboard_button),
            executable_path: overlay
                .executable_path
                .clone()
                .or_else(|| base.executable_path.clone()),
        },
    }
}

fn merge_capabilities(
    base: &std::collections::BTreeMap<String, bool>,
    overlay: Option<&std::collections::BTreeMap<String, bool>>,
) -> std::collections::BTreeMap<String, bool> {
    let mut merged = base.clone();
    if let Some(overlay) = overlay {
        merged.extend(
            overlay
                .iter()
                .map(|(name, enabled)| (name.clone(), *enabled)),
        );
    }
    merged
}

fn merge_appearance(
    base: &AppearanceSettings,
    overlay: Option<&PartialAppearanceSettings>,
) -> AppearanceSettings {
    match overlay {
        None => base.clone(),
        Some(overlay) => AppearanceSettings {
            theme_mode: overlay
                .theme_mode
                .clone()
                .unwrap_or_else(|| base.theme_mode.clone()),
            font_size: overlay.font_size.unwrap_or(base.font_size),
            translucent_sidebar: overlay
                .translucent_sidebar
                .unwrap_or(base.translucent_sidebar),
            show_rate_limits: overlay.show_rate_limits.unwrap_or(base.show_rate_limits),
            show_hourly_rate_limit: overlay
                .show_hourly_rate_limit
                .unwrap_or(base.show_hourly_rate_limit),
            show_weekly_rate_limit: overlay
                .show_weekly_rate_limit
                .unwrap_or(base.show_weekly_rate_limit),
            rate_limit_display_mode: overlay
                .rate_limit_display_mode
                .clone()
                .unwrap_or_else(|| base.rate_limit_display_mode.clone()),
            light_theme: overlay
                .light_theme
                .clone()
                .unwrap_or_else(|| base.light_theme.clone()),
            dark_theme: overlay
                .dark_theme
                .clone()
                .unwrap_or_else(|| base.dark_theme.clone()),
        },
    }
}

fn merge_compaction(
    base: &CompactionSettings,
    overlay: Option<&PartialCompactionSettings>,
) -> CompactionSettings {
    match overlay {
        None => base.clone(),
        Some(o) => CompactionSettings {
            enabled: o.enabled.unwrap_or(base.enabled),
            trigger_ratio: o.trigger_ratio.unwrap_or(base.trigger_ratio),
            target_ratio: o.target_ratio.unwrap_or(base.target_ratio),
        },
    }
}

fn merge_retry(base: &RetrySettings, overlay: Option<&PartialRetrySettings>) -> RetrySettings {
    match overlay {
        None => base.clone(),
        Some(o) => RetrySettings {
            timeout_ms: o.timeout_ms.or(base.timeout_ms),
            max_retries: o.max_retries.or(base.max_retries),
            max_retry_delay_ms: o.max_retry_delay_ms.or(base.max_retry_delay_ms),
        },
    }
}

fn merge_permissions(
    base: &PermissionSettings,
    overlay: Option<&PartialPermissionSettings>,
) -> PermissionSettings {
    match overlay {
        None => base.clone(),
        Some(o) => PermissionSettings {
            deny: o.deny.clone().unwrap_or_else(|| base.deny.clone()),
            ask: o.ask.clone().unwrap_or_else(|| base.ask.clone()),
            allow: o
                .allow
                .clone()
                .map(super::schema::migrate_permission_allow_rules)
                .unwrap_or_else(|| base.allow.clone()),
            mode: o.mode.clone().unwrap_or_else(|| base.mode.clone()),
        },
    }
}
