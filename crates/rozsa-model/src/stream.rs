//! Public stream entry points backed by the provider registry.

use crate::event_stream::EventStream;
use crate::registry::get_provider;
use crate::types::{Context, Model, SimpleStreamOptions, StreamOptions};

pub type StreamEvent = crate::types::StreamEvent;

/// Stream a provider-specific request through the registered API provider.
pub fn stream(
    model: &Model,
    context: &Context,
    options: &StreamOptions,
) -> EventStream<StreamEvent> {
    let provider = get_provider(&model.api)
        .unwrap_or_else(|| panic!("No provider registered for api: {:?}", model.api));
    provider.stream(model, context, options)
}

/// Stream a request through the registered API provider using unified options.
pub fn stream_simple(
    model: &Model,
    context: &Context,
    options: &SimpleStreamOptions,
) -> EventStream<StreamEvent> {
    let provider = get_provider(&model.api)
        .unwrap_or_else(|| panic!("No provider registered for api: {:?}", model.api));
    provider.stream_simple(model, context, options)
}
