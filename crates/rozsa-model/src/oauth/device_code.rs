use std::time::Duration;
use tokio::time::{interval, timeout};
use tokio_util::sync::CancellationToken;

/// Result of a single poll attempt.
pub enum DeviceCodePollResult {
    Pending,
    Complete { access_token: String },
    SlowDown,
    Failed { message: String },
}

/// Poll for device code authorization completion.
/// Implements RFC 8628 polling with interval, slow_down handling, and timeout.
pub async fn poll_device_code<F, Fut>(
    interval_seconds: u64,
    expires_in_seconds: u64,
    poll_fn: F,
    cancel: CancellationToken,
) -> Result<String, super::types::OAuthLoginError>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = DeviceCodePollResult>,
{
    let mut current_interval = Duration::from_secs(interval_seconds);
    let total_timeout = Duration::from_secs(expires_in_seconds);

    let result = timeout(total_timeout, async {
        let mut ticker = interval(current_interval);
        ticker.tick().await; // First tick completes immediately

        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    match poll_fn().await {
                        DeviceCodePollResult::Complete { access_token } => {
                            return Ok(access_token);
                        }
                        DeviceCodePollResult::Pending => {
                            // Continue polling
                        }
                        DeviceCodePollResult::SlowDown => {
                            current_interval += Duration::from_secs(5);
                            ticker = interval(current_interval);
                            ticker.tick().await; // Reset
                        }
                        DeviceCodePollResult::Failed { message } => {
                            return Err(super::types::OAuthLoginError::Provider(message));
                        }
                    }
                }
                _ = cancel.cancelled() => {
                    return Err(super::types::OAuthLoginError::Cancelled);
                }
            }
        }
    })
    .await;

    match result {
        Ok(inner) => inner,
        Err(_) => Err(super::types::OAuthLoginError::Timeout),
    }
}
