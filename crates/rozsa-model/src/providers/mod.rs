//! Built-in provider modules and registration helpers.

pub mod anthropic;
pub mod bedrock;
pub mod common;
pub mod openai_completions;
pub mod openai_responses;

use crate::registry::register_provider;

/// Register provider implementations that are ready for program use.
pub fn register_builtin_providers() {
    register_provider(Box::new(
        openai_completions::OpenAICompletionsProvider::new(),
    ));
    register_provider(Box::new(
        openai_responses::OpenAIResponsesProvider::new(),
    ));
    register_provider(Box::new(bedrock::BedrockProvider::new()));
    register_provider(Box::new(anthropic::AnthropicProvider::new()));
}
