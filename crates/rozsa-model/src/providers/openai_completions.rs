//! OpenAI-compatible Chat Completions provider implementation.

#[path = "openai_completions/payload.rs"]
mod payload;
#[path = "openai_completions/sse.rs"]
pub mod sse;

use std::collections::HashMap;
use std::time::Duration;

use futures_util::StreamExt;
use serde_json::Value;

use crate::event_stream::{EventStream, EventStreamSender, create_event_stream};
use crate::providers::common::{
    ProviderError, ProviderResult, build_http_client, calculate_cost, create_output, emit_error,
    join_url, map_finish_reason, merge_headers, resolve_api_key, to_header_map,
};
use crate::registry::ApiProvider;
use crate::types::{
    Api, AssistantMessage, ContentBlock, Context, Message, Model, Provider, SimpleStreamOptions,
    StopReason, StreamEvent, StreamOptions, ToolCall, Usage,
};

pub use payload::{
    MaxTokensField, ThinkingFormat, build_chat_completions_payload, convert_messages,
    convert_tools, resolve_compat,
};
pub use sse::{SseParser, parse_sse_chunks};

/// Provider for OpenAI Chat Completions and compatible HTTP/SSE APIs.
pub struct OpenAICompletionsProvider {
    api: Api,
}

impl OpenAICompletionsProvider {
    /// Create a provider instance that handles the `OpenAICompletions` API.
    pub fn new() -> Self {
        Self {
            api: Api::OpenAICompletions,
        }
    }
}

impl Default for OpenAICompletionsProvider {
    /// Create the default OpenAI-compatible provider instance.
    fn default() -> Self {
        Self::new()
    }
}

impl ApiProvider for OpenAICompletionsProvider {
    /// Return the API protocol handled by this provider.
    fn api(&self) -> &Api {
        &self.api
    }

    /// Stream with provider options by adapting them to simple options.
    fn stream(
        &self,
        model: &Model,
        context: &Context,
        options: &StreamOptions,
    ) -> EventStream<StreamEvent> {
        let simple_options = SimpleStreamOptions {
            base: options.clone(),
            reasoning: None,
            thinking_budgets: None,
            tool_choice: None,
        };
        self.stream_simple(model, context, &simple_options)
    }

    /// Stream using unified options and emit normalized provider events.
    fn stream_simple(
        &self,
        model: &Model,
        context: &Context,
        options: &SimpleStreamOptions,
    ) -> EventStream<StreamEvent> {
        let (sender, stream) = create_event_stream();
        let model = model.clone();
        let context = context.clone();
        let options = options.clone();
        tokio::spawn(async move {
            let output = create_output(&model, Api::OpenAICompletions);
            match stream_openai_chat_response(&model, &context, &options, output, &sender).await {
                Ok(()) => {}
                Err(error) => emit_error(
                    &sender,
                    create_output(&model, Api::OpenAICompletions),
                    error,
                ),
            }
        });
        stream
    }
}

/// Execute one OpenAI-compatible stream request and return normalized events.
pub async fn run_openai_chat_stream(
    model: &Model,
    context: &Context,
    options: &SimpleStreamOptions,
    output: AssistantMessage,
) -> ProviderResult<Vec<StreamEvent>> {
    let (sender, mut stream) = create_event_stream();
    stream_openai_chat_response(model, context, options, output, &sender).await?;
    drop(sender);
    let mut events = Vec::new();
    while let Some(event) = stream.next().await {
        events.push(event);
    }
    Ok(events)
}

/// Execute one stream request and emit normalized events as chunks arrive.
pub async fn stream_openai_chat_response(
    model: &Model,
    context: &Context,
    options: &SimpleStreamOptions,
    output: AssistantMessage,
    sender: &EventStreamSender<StreamEvent>,
) -> ProviderResult<()> {
    let api_key = resolve_api_key(model, &options.base)?;
    let client = build_http_client(&options.base)?;
    let base_url = resolve_base_url(model)?;
    let url = join_url(&base_url, "chat/completions")?;
    let payload = build_chat_completions_payload(model, context, options);
    let headers = request_headers(model, context, options, &api_key)?;

    let response = send_with_retries(&client, &url, &headers, &payload, options).await?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(ProviderError::HttpStatus { status, body });
    }

    stream_response_events(model, output, response, sender).await
}

/// Build request headers from model defaults, request options, auth, and session settings.
pub fn request_headers(
    model: &Model,
    context: &Context,
    options: &SimpleStreamOptions,
    api_key: &str,
) -> ProviderResult<reqwest::header::HeaderMap> {
    let mut headers = merge_headers(model.headers.as_ref(), options.base.headers.as_ref());
    if is_cloudflare_ai_gateway(&model.provider) {
        headers
            .entry("cf-aig-authorization".to_string())
            .or_insert_with(|| format!("Bearer {api_key}"));
        headers.remove("Authorization");
    } else {
        headers
            .entry("Authorization".to_string())
            .or_insert_with(|| format!("Bearer {api_key}"));
    }
    headers
        .entry("Content-Type".to_string())
        .or_insert_with(|| "application/json".to_string());
    apply_copilot_headers(model, context, &mut headers);
    let compat = resolve_compat(model);
    if let Some(session_id) = &options.base.session_id {
        if compat.send_session_affinity_headers {
            headers.insert("session_id".to_string(), session_id.clone());
            headers.insert("x-client-request-id".to_string(), session_id.clone());
            headers.insert("x-session-affinity".to_string(), session_id.clone());
        }
    }
    to_header_map(&headers)
}

/// Send the request with retry behavior for transient errors.
async fn send_with_retries(
    client: &reqwest::Client,
    url: &str,
    headers: &reqwest::header::HeaderMap,
    payload: &Value,
    options: &SimpleStreamOptions,
) -> ProviderResult<reqwest::Response> {
    let max_retries = options.base.max_retries.unwrap_or(2);
    let mut attempt = 0;
    loop {
        let result = client
            .post(url)
            .headers(headers.clone())
            .json(payload)
            .send()
            .await;

        match result {
            Ok(response) if !should_retry_status(response.status()) || attempt >= max_retries => {
                return Ok(response);
            }
            Ok(response) => {
                let delay = retry_delay(response.headers(), attempt, options);
                attempt += 1;
                tokio::time::sleep(delay).await;
            }
            Err(error) if should_retry_error(&error) && attempt < max_retries => {
                let delay = retry_delay_empty(attempt, options);
                attempt += 1;
                tokio::time::sleep(delay).await;
            }
            Err(error) => return Err(ProviderError::Http(error)),
        }
    }
}

/// Decide whether a status code is normally retryable.
pub fn should_retry_status(status: reqwest::StatusCode) -> bool {
    status == reqwest::StatusCode::REQUEST_TIMEOUT
        || status == reqwest::StatusCode::CONFLICT
        || status == reqwest::StatusCode::TOO_MANY_REQUESTS
        || status.is_server_error()
}

/// Decide whether a transport error is normally retryable.
fn should_retry_error(error: &reqwest::Error) -> bool {
    error.is_timeout() || error.is_connect()
}

/// Compute retry delay from headers or exponential fallback.
fn retry_delay(
    headers: &reqwest::header::HeaderMap,
    attempt: u32,
    options: &SimpleStreamOptions,
) -> Duration {
    let from_header = headers
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .map(Duration::from_secs);
    clamp_retry_delay(
        from_header.unwrap_or_else(|| retry_delay_empty(attempt, options)),
        options,
    )
}

/// Compute exponential retry delay when the provider does not request one.
fn retry_delay_empty(attempt: u32, options: &SimpleStreamOptions) -> Duration {
    let millis = 250_u64.saturating_mul(2_u64.saturating_pow(attempt.min(6)));
    clamp_retry_delay(Duration::from_millis(millis), options)
}

/// Apply max retry delay cap when configured.
fn clamp_retry_delay(delay: Duration, options: &SimpleStreamOptions) -> Duration {
    let Some(max_ms) = options.base.max_retry_delay_ms else {
        return delay;
    };
    delay.min(Duration::from_millis(max_ms))
}

/// Resolve provider base URL placeholders such as Cloudflare account IDs.
fn resolve_base_url(model: &Model) -> ProviderResult<String> {
    let mut output = String::new();
    let mut chars = model.base_url.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '{' {
            output.push(ch);
            continue;
        }
        let mut name = String::new();
        for next in chars.by_ref() {
            if next == '}' {
                break;
            }
            name.push(next);
        }
        let value = std::env::var(&name).map_err(|_| ProviderError::InvalidUrl {
            url: model.base_url.clone(),
            message: format!(
                "{name} is required for provider {}",
                provider_name(&model.provider)
            ),
        })?;
        output.push_str(&value);
    }
    Ok(output)
}

/// Add GitHub Copilot headers that depend on request messages.
fn apply_copilot_headers(model: &Model, context: &Context, headers: &mut HashMap<String, String>) {
    if !is_github_copilot(&model.provider) {
        return;
    }
    headers
        .entry("X-Initiator".to_string())
        .or_insert_with(|| copilot_initiator(context).to_string());
    headers
        .entry("Openai-Intent".to_string())
        .or_insert_with(|| "conversation-edits".to_string());
    if has_vision_input(context) {
        headers
            .entry("Copilot-Vision-Request".to_string())
            .or_insert_with(|| "true".to_string());
    }
}

/// Return whether this provider is GitHub Copilot.
fn is_github_copilot(provider: &Provider) -> bool {
    matches!(provider, Provider::Custom(value) if value == "github-copilot")
}

/// Return whether this provider is Cloudflare AI Gateway.
fn is_cloudflare_ai_gateway(provider: &Provider) -> bool {
    matches!(provider, Provider::CloudflareAIGateway)
        || matches!(provider, Provider::Custom(value) if value == "cloudflare-ai-gateway")
}

/// Return a stable provider name for error messages.
fn provider_name(provider: &Provider) -> String {
    match provider {
        Provider::Custom(value) => value.clone(),
        other => format!("{other:?}"),
    }
}

/// Infer the Copilot initiator header from the last conversation message.
fn copilot_initiator(context: &Context) -> &'static str {
    match context.messages.last() {
        Some(Message::User(_)) | None => "user",
        _ => "agent",
    }
}

/// Return whether messages include image input for Copilot.
fn has_vision_input(context: &Context) -> bool {
    context.messages.iter().any(|message| match message {
        Message::User(user) => match &user.content {
            crate::types::UserContent::Blocks(blocks) => blocks
                .iter()
                .any(|block| matches!(block, ContentBlock::Image { .. })),
            crate::types::UserContent::Text(_) => false,
        },
        Message::ToolResult(tool) => tool
            .content
            .iter()
            .any(|block| matches!(block, ContentBlock::Image { .. })),
        Message::Assistant(_) => false,
    })
}

/// Parse HTTP bytes incrementally and push normalized stream events immediately.
async fn stream_response_events(
    model: &Model,
    output: AssistantMessage,
    response: reqwest::Response,
    sender: &EventStreamSender<StreamEvent>,
) -> ProviderResult<()> {
    let mut stream = response.bytes_stream();
    let mut bytes_buffer = Vec::new();
    let mut parser = sse::SseParser::new();
    let mut normalizer = ChatStreamNormalizer::new(model, output);
    for event in normalizer.start() {
        sender.push(event);
    }

    while let Some(chunk) = stream.next().await {
        bytes_buffer.extend_from_slice(&chunk?);
        let text = match std::str::from_utf8(&bytes_buffer) {
            Ok(text) => {
                let text = text.to_string();
                bytes_buffer.clear();
                text
            }
            Err(error) if error.error_len().is_none() => {
                let valid_up_to = error.valid_up_to();
                if valid_up_to == 0 {
                    continue;
                }
                let text = std::str::from_utf8(&bytes_buffer[..valid_up_to])
                    .map_err(|error| {
                        ProviderError::Parse(format!("stream was not utf-8: {error}"))
                    })?
                    .to_string();
                bytes_buffer.drain(..valid_up_to);
                text
            }
            Err(error) => {
                return Err(ProviderError::Parse(format!(
                    "stream was not utf-8: {error}"
                )));
            }
        };
        for chunk in parser.feed(&text)? {
            for event in normalizer.push_chunk(chunk) {
                sender.push(event);
            }
        }
    }

    if !bytes_buffer.is_empty() {
        let text = std::str::from_utf8(&bytes_buffer)
            .map_err(|error| ProviderError::Parse(format!("stream was not utf-8: {error}")))?;
        for chunk in parser.feed(text)? {
            for event in normalizer.push_chunk(chunk) {
                sender.push(event);
            }
        }
    }
    for chunk in parser.finish()? {
        for event in normalizer.push_chunk(chunk) {
            sender.push(event);
        }
    }
    for event in normalizer.finish()? {
        sender.push(event);
    }
    Ok(())
}

/// Convert parsed OpenAI-compatible chunks into normalized stream events.
pub fn normalize_chat_chunks(
    model: &Model,
    output: AssistantMessage,
    chunks: Vec<sse::ChatChunk>,
) -> ProviderResult<Vec<StreamEvent>> {
    let mut normalizer = ChatStreamNormalizer::new(model, output);
    let mut events = normalizer.start();
    for chunk in chunks {
        events.extend(normalizer.push_chunk(chunk));
    }
    events.extend(normalizer.finish()?);
    Ok(events)
}

/// Incrementally normalizes parsed chat chunks into stream events.
pub struct ChatStreamNormalizer<'a> {
    model: &'a Model,
    output: AssistantMessage,
    state: StreamState,
    started: bool,
    has_finish_reason: bool,
}

impl<'a> ChatStreamNormalizer<'a> {
    /// Create normalizer state for one provider response.
    pub fn new(model: &'a Model, output: AssistantMessage) -> Self {
        Self {
            model,
            output,
            state: StreamState::default(),
            started: false,
            has_finish_reason: false,
        }
    }

    /// Emit the initial start event once.
    pub fn start(&mut self) -> Vec<StreamEvent> {
        if self.started {
            return Vec::new();
        }
        self.started = true;
        vec![StreamEvent::Start {
            partial: self.output.clone(),
        }]
    }

    /// Apply one parsed provider chunk and return newly available events.
    pub fn push_chunk(&mut self, chunk: sse::ChatChunk) -> Vec<StreamEvent> {
        let mut events = Vec::new();
        if let Some(id) = chunk.id {
            self.output.response_id.get_or_insert(id);
        }
        if let Some(response_model) = chunk.model.filter(|value| value != &self.model.id) {
            self.output.response_model.get_or_insert(response_model);
        }
        if let Some(usage) = chunk.usage {
            self.output.usage = parse_usage(self.model, usage);
        }
        let Some(choice) = chunk.choices.into_iter().next() else {
            return events;
        };
        if self.output.usage.total_tokens == 0 {
            if let Some(usage) = choice.usage {
                self.output.usage = parse_usage(self.model, usage);
            }
        }
        if let Some(reason) = choice.finish_reason.as_deref() {
            let (stop_reason, error_message) = map_finish_reason(Some(reason));
            self.output.stop_reason = stop_reason;
            self.output.error_message = error_message;
            self.has_finish_reason = true;
        }
        apply_delta(&mut self.state, &mut self.output, choice.delta, &mut events);
        events
    }

    /// Finish open content blocks and emit the terminal event.
    pub fn finish(&mut self) -> ProviderResult<Vec<StreamEvent>> {
        let mut events = Vec::new();
        finish_open_blocks(&mut self.state, &mut self.output, &mut events);
        if self.output.stop_reason == StopReason::Error {
            return Err(ProviderError::Parse(
                self.output
                    .error_message
                    .clone()
                    .unwrap_or_else(|| "Provider returned an error stop reason".to_string()),
            ));
        }
        if !self.has_finish_reason {
            return Err(ProviderError::Parse(
                "Stream ended without finish_reason".to_string(),
            ));
        }
        events.push(StreamEvent::Done {
            reason: self.output.stop_reason,
            message: self.output.clone(),
        });
        Ok(events)
    }
}

#[derive(Default)]
/// Tracks open content blocks while a provider stream is normalized.
struct StreamState {
    text_index: Option<usize>,
    thinking_index: Option<usize>,
    tool_call_indices: HashMap<usize, usize>,
    tool_call_args: HashMap<usize, String>,
}

/// Apply one streamed delta to the partial message and append matching events.
fn apply_delta(
    state: &mut StreamState,
    output: &mut AssistantMessage,
    delta: sse::Delta,
    events: &mut Vec<StreamEvent>,
) {
    let content_delta = delta.content.clone().filter(|value| !value.is_empty());
    if let Some(content) = content_delta {
        let index = ensure_text_block(state, output, events);
        if let Some(ContentBlock::Text { text, .. }) = output.content.get_mut(index) {
            text.push_str(&content);
        }
        events.push(StreamEvent::TextDelta {
            content_index: index,
            delta: content,
            partial: output.clone(),
        });
    }

    if let Some(reasoning) = first_reasoning_delta(&delta) {
        let index = ensure_thinking_block(state, output, events);
        if let Some(ContentBlock::Thinking { thinking, .. }) = output.content.get_mut(index) {
            thinking.push_str(&reasoning);
        }
        events.push(StreamEvent::ThinkingDelta {
            content_index: index,
            delta: reasoning,
            partial: output.clone(),
        });
    }

    if let Some(tool_calls) = delta.tool_calls {
        for tool_call in tool_calls {
            let stream_index = tool_call.index.unwrap_or(0);
            let content_index = ensure_tool_call_block(state, output, events, stream_index);
            if let Some(ContentBlock::ToolCall(block)) = output.content.get_mut(content_index) {
                if let Some(id) = tool_call.id.filter(|value| !value.is_empty()) {
                    block.id = id;
                }
                if let Some(function) = tool_call.function {
                    if let Some(name) = function.name.filter(|value| !value.is_empty()) {
                        block.name = name;
                    }
                    if let Some(arguments) = function.arguments {
                        let args = state.tool_call_args.entry(stream_index).or_default();
                        args.push_str(&arguments);
                        block.arguments = parse_json_or_string(args);
                        events.push(StreamEvent::ToolCallDelta {
                            content_index,
                            delta: arguments,
                            partial: output.clone(),
                        });
                    }
                }
            }
        }
    }
}

/// Ensure a text block exists and emit `TextStart` when it is created.
fn ensure_text_block(
    state: &mut StreamState,
    output: &mut AssistantMessage,
    events: &mut Vec<StreamEvent>,
) -> usize {
    if let Some(index) = state.text_index {
        return index;
    }
    let index = output.content.len();
    output.content.push(ContentBlock::Text {
        text: String::new(),
        signature: None,
    });
    state.text_index = Some(index);
    events.push(StreamEvent::TextStart {
        content_index: index,
        partial: output.clone(),
    });
    index
}

/// Ensure a thinking block exists and emit `ThinkingStart` when it is created.
fn ensure_thinking_block(
    state: &mut StreamState,
    output: &mut AssistantMessage,
    events: &mut Vec<StreamEvent>,
) -> usize {
    if let Some(index) = state.thinking_index {
        return index;
    }
    let index = output.content.len();
    output.content.push(ContentBlock::Thinking {
        thinking: String::new(),
        signature: Some("reasoning_content".to_string()),
        redacted: false,
    });
    state.thinking_index = Some(index);
    events.push(StreamEvent::ThinkingStart {
        content_index: index,
        partial: output.clone(),
    });
    index
}

/// Ensure a tool-call block exists for a provider stream index.
fn ensure_tool_call_block(
    state: &mut StreamState,
    output: &mut AssistantMessage,
    events: &mut Vec<StreamEvent>,
    stream_index: usize,
) -> usize {
    if let Some(index) = state.tool_call_indices.get(&stream_index) {
        return *index;
    }
    let index = output.content.len();
    output.content.push(ContentBlock::ToolCall(ToolCall {
        id: String::new(),
        name: String::new(),
        arguments: Value::Object(Default::default()),
    }));
    state.tool_call_indices.insert(stream_index, index);
    events.push(StreamEvent::ToolCallStart {
        content_index: index,
        partial: output.clone(),
    });
    index
}

/// Emit end events for all content blocks opened during streaming.
fn finish_open_blocks(
    state: &mut StreamState,
    output: &mut AssistantMessage,
    events: &mut Vec<StreamEvent>,
) {
    if let Some(index) = state.text_index {
        if let Some(ContentBlock::Text { text, .. }) = output.content.get(index) {
            events.push(StreamEvent::TextEnd {
                content_index: index,
                content: text.clone(),
                partial: output.clone(),
            });
        }
    }
    if let Some(index) = state.thinking_index {
        if let Some(ContentBlock::Thinking { thinking, .. }) = output.content.get(index) {
            events.push(StreamEvent::ThinkingEnd {
                content_index: index,
                content: thinking.clone(),
                partial: output.clone(),
            });
        }
    }
    for index in state.tool_call_indices.values() {
        if let Some(ContentBlock::ToolCall(tool_call)) = output.content.get(*index) {
            events.push(StreamEvent::ToolCallEnd {
                content_index: *index,
                tool_call: tool_call.clone(),
                partial: output.clone(),
            });
        }
    }
}

/// Return the first non-empty reasoning delta field from a chunk delta.
fn first_reasoning_delta(delta: &sse::Delta) -> Option<String> {
    delta
        .reasoning_content
        .clone()
        .or_else(|| delta.reasoning.clone())
        .or_else(|| delta.reasoning_text.clone())
        .filter(|value| !value.is_empty())
}

/// Parse partial tool-call JSON, preserving raw text until it becomes valid JSON.
fn parse_json_or_string(raw: &str) -> Value {
    serde_json::from_str(raw).unwrap_or_else(|_| Value::String(raw.to_string()))
}

/// Convert provider usage fields into normalized usage and model cost.
fn parse_usage(model: &Model, raw: sse::RawUsage) -> Usage {
    let prompt_tokens = raw.prompt_tokens.unwrap_or(0);
    let cache_read = raw
        .prompt_tokens_details
        .as_ref()
        .and_then(|details| details.cached_tokens)
        .or(raw.prompt_cache_hit_tokens)
        .unwrap_or(0);
    let cache_write = raw
        .prompt_tokens_details
        .as_ref()
        .and_then(|details| details.cache_write_tokens)
        .unwrap_or(0);
    let output = raw.completion_tokens.unwrap_or(0);
    let input = prompt_tokens.saturating_sub(cache_read + cache_write);
    let mut usage = Usage {
        input,
        output,
        cache_read,
        cache_write,
        total_tokens: input + output + cache_read + cache_write,
        cost: crate::providers::common::empty_usage().cost,
    };
    calculate_cost(model, &mut usage);
    usage
}
