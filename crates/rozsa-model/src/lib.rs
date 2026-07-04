//! Rust model layer for provider-agnostic LLM streaming.

pub mod credentials;
pub mod env_keys;
pub mod event_stream;
pub mod http_client;
pub mod oauth;
pub mod providers;
pub mod registry;
pub mod stream;
pub mod types;
mod types_serde;
