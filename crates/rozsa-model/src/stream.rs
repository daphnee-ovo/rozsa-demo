//! Public stream entry points backed by the provider registry.

use crate::event_stream::{EventStream, create_event_stream};
use crate::registry::get_provider;
use crate::types::{
    Api, AssistantMessage, Context, Model, Provider, SimpleStreamOptions, StopReason,
    StreamOptions, Usage, UsageCost,
};

pub type StreamEvent = crate::types::StreamEvent;

/// Stream a provider-specific request through the registered API provider.
pub fn stream(
    model: &Model,
    context: &Context,
    options: &StreamOptions,
) -> EventStream<StreamEvent> {
    let Some(provider) = get_provider(&model.api) else {
        return emit_unsupported_error(&model.api, &model.provider, &model.id);
    };
    provider.stream(model, context, options)
}

/// Stream a request through the registered API provider using unified options.
pub fn stream_simple(
    model: &Model,
    context: &Context,
    options: &SimpleStreamOptions,
) -> EventStream<StreamEvent> {
    let Some(provider) = get_provider(&model.api) else {
        return emit_unsupported_error(&model.api, &model.provider, &model.id);
    };
    provider.stream_simple(model, context, options)
}

fn emit_unsupported_error(
    api: &Api,
    provider: &Provider,
    model_id: &str,
) -> EventStream<StreamEvent> {
    let (tx, rx) = create_event_stream();
    let error_msg = format!(
        "Provider {:?} (api: {:?}) is not yet implemented. Model '{}' cannot be used.",
        provider, api, model_id
    );
    let error = AssistantMessage {
        content: vec![],
        api: api.clone(),
        provider: provider.clone(),
        model: model_id.to_string(),
        response_model: None,
        response_id: None,
        usage: Usage {
            input: 0,
            output: 0,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
            cost: UsageCost {
                input: 0.0,
                output: 0.0,
                cache_read: 0.0,
                cache_write: 0.0,
                total: 0.0,
            },
        },
        stop_reason: StopReason::Error,
        error_message: Some(error_msg),
        timestamp: 0,
    };
    tx.push(StreamEvent::Error {
        reason: StopReason::Error,
        error,
    });
    rx
}
