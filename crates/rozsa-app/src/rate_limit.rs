// App-layer rate limit query for codex-oauth subscription accounts.
//
// Internal Framework:
// rate_limit.rs (app layer)
// ├── get_rate_limits()           — main entry: resolve auth path → fetch
// └── format_rate_limit_display() — format for TUI display
//
// Related Docs:
// - [Rate Limit module](../../rozsa-model/src/rate_limit.rs)

use rozsa_model::rate_limit::{RateLimitError, RateLimitSnapshot};

/// Get rate limits using default auth.json path (~/.rozsa/models/auth.json).
pub async fn get_rate_limits() -> Result<RateLimitSnapshot, RateLimitError> {
    let auth_path = crate::config_paths::ConfigRoots::global_models_dir()
        .map_err(|error| RateLimitError::MissingCredentials(error.to_string()))?
        .join("auth.json");
    let path_str = auth_path.to_string_lossy().to_string();
    rozsa_model::rate_limit::fetch_rate_limits_from_auth(&path_str).await
}

/// Format rate limit snapshot for display in TUI status bar or notification.
pub fn format_rate_limit_display(snapshot: &RateLimitSnapshot) -> String {
    let mut parts = Vec::new();

    if let Some(ref plan) = snapshot.plan_type {
        parts.push(format!("Plan: {plan}"));
    }

    if let Some(ref primary) = snapshot.primary {
        let window_hours = primary.window_duration_secs / 3600;
        let reset_mins = primary.reset_after_secs / 60;
        parts.push(format!(
            "{}h window: {:.0}% used (resets in {}m)",
            window_hours, primary.used_percent, reset_mins
        ));
    }

    if let Some(ref secondary) = snapshot.secondary {
        let window_days = secondary.window_duration_secs / 86400;
        let reset_hours = secondary.reset_after_secs / 3600;
        parts.push(format!(
            "{}d window: {:.0}% used (resets in {}h)",
            window_days, secondary.used_percent, reset_hours
        ));
    }

    if snapshot.limit_reached {
        parts.push("Rate limit reached".to_string());
    }

    if parts.is_empty() {
        "No rate limit data available".to_string()
    } else {
        parts.join(" | ")
    }
}
