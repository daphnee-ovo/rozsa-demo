use super::schema::{
    CompactionSettings, PartialCompactionSettings, PartialPermissionSettings,
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
        default_thinking_level: overlay
            .default_thinking_level
            .clone()
            .or_else(|| base.default_thinking_level.clone()),
        compaction: merge_compaction(&base.compaction, overlay.compaction.as_ref()),
        retry: merge_retry(&base.retry, overlay.retry.as_ref()),
        transport: overlay
            .transport
            .clone()
            .unwrap_or_else(|| base.transport.clone()),
        block_images: overlay.block_images.unwrap_or(base.block_images),
        hide_thinking: overlay.hide_thinking.unwrap_or(base.hide_thinking),
        steering_mode: overlay
            .steering_mode
            .clone()
            .unwrap_or_else(|| base.steering_mode.clone()),
        follow_up_mode: overlay
            .follow_up_mode
            .clone()
            .unwrap_or_else(|| base.follow_up_mode.clone()),
        permissions: merge_permissions(&base.permissions, overlay.permissions.as_ref()),
        context_window_preferences: overlay
            .context_window_preferences
            .clone()
            .unwrap_or_else(|| base.context_window_preferences.clone()),
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
            threshold_tokens: o.threshold_tokens.unwrap_or(base.threshold_tokens),
            target_tokens: o.target_tokens.unwrap_or(base.target_tokens),
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
            mode: o.mode.clone().unwrap_or_else(|| base.mode.clone()),
            auto_approve_patterns: o
                .auto_approve_patterns
                .clone()
                .unwrap_or_else(|| base.auto_approve_patterns.clone()),
        },
    }
}
