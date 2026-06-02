//! Built-in provider modules and registration helpers.

pub mod anthropic;
pub mod common;
pub mod faux;
pub mod openai_completions;

use crate::registry::register_provider;

/// Register provider implementations that are ready for program use.
pub fn register_builtin_providers() {
    register_provider(Box::new(
        openai_completions::OpenAICompletionsProvider::new(),
    ));
}
