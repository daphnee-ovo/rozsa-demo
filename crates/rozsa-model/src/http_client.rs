//! Shared HTTP client with proxy support for all providers.
//!
//! Internal Framework:
//! http_client.rs
//! ├── SHARED_CLIENT — lazy-initialized static Client
//! ├── build_client() — construct Client with proxy + timeout
//! └── shared_client() — accessor to the global Client
//!
//! Related Docs:
//! - [TASK-T035](../../../.dev-doc/main/task/task_2026-07-03_1.md#TASK-T035)

use once_cell::sync::Lazy;
use reqwest::Client;
use std::time::Duration;

/// Globally shared HTTP client with proxy and timeout configuration.
static SHARED_CLIENT: Lazy<Client> = Lazy::new(build_client);

/// Build a reqwest Client with standard timeouts and proxy support.
///
/// Respects `HTTP_PROXY` and `HTTPS_PROXY` environment variables.
/// Falls back to a default client if proxy configuration fails.
fn build_client() -> Client {
    let mut builder = Client::builder()
        .timeout(Duration::from_secs(300))
        .connect_timeout(Duration::from_secs(30))
        .pool_idle_timeout(Duration::from_secs(90));

    // Respect HTTP_PROXY / HTTPS_PROXY environment variables.
    // reqwest automatically reads these env vars when using default builder,
    // but we make it explicit for clarity.
    if let Ok(proxy_url) = std::env::var("HTTPS_PROXY").or_else(|_| std::env::var("HTTP_PROXY")) &&
       let Ok(proxy) = reqwest::Proxy::all(&proxy_url) {
        builder = builder.proxy(proxy);
    }

    builder.build().unwrap_or_else(|_| Client::new())
}

/// Get the shared HTTP client configured with proxy and timeouts.
///
/// This client is initialized once on first access and reused across all provider requests.
pub fn shared_client() -> &'static Client {
    &SHARED_CLIENT
}
