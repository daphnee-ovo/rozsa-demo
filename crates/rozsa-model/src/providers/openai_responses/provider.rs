// FrameworkTree
// provider.rs
// ├── struct OpenAIResponsesProvider
// ├── impl OpenAIResponsesProvider
// ├── new()
// ├── impl OpenAIResponsesProvider
// ├── default()
// ├── impl OpenAIResponsesProvider
// ├── api()
// ├── stream()
// ├── stream_simple()
// ├── thinking_effort_to_effort()
// └── stream_responses()

// Provider for OpenAI Responses API (POST /v1/responses).
//
// Internal Framework:
// provider.rs
// ├── OpenAIResponsesProvider       — ApiProvider impl for OpenAIResponses
// │   ├── stream()
// │   └── stream_simple()
// └── stream_responses()            — core streaming logic
//
// Reference:
// - codex-rs core/src/client.rs (stream_responses_api, lines 1357-1466)
// - crates/rozsa-model/src/providers/openai_completions.rs (pattern reference)
//
// Related Docs:
// - [Responses API types](./types.rs)
// - [Responses SSE parser](./sse.rs)
// - [Responses converter](./convert.rs)

use futures_util::StreamExt;

use crate::event_stream::{EventStream, EventStreamSender, create_event_stream};
use crate::providers::common::{
    ProviderError, build_http_client, create_output, emit_error, join_url, merge_headers,
    resolve_api_key, to_header_map,
};
use crate::registry::ApiProvider;
use crate::types::{
    Api, AssistantMessage, Context, Model, SimpleStreamOptions, StreamEvent, StreamOptions,
    ThinkingEffort,
};

use super::convert::{ResponseStreamNormalizer, convert_messages, convert_tools};
use super::sse::ResponsesSseParser;
use super::types::{Reasoning, ResponsesApiRequest};

/// Provider for the OpenAI Responses API protocol (`POST /v1/responses`).
pub struct OpenAIResponsesProvider {
    api: Api,
}

impl OpenAIResponsesProvider {
    pub fn new() -> Self {
        Self {
            api: Api::OpenAIResponses,
        }
    }
}

impl Default for OpenAIResponsesProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl ApiProvider for OpenAIResponsesProvider {
    fn api(&self) -> &Api {
        &self.api
    }

    fn stream(
        &self,
        model: &Model,
        context: &Context,
        options: &StreamOptions,
    ) -> EventStream<StreamEvent> {
        let simple_options = SimpleStreamOptions {
            base: options.clone(),
            reasoning: None,
            thinking_effort_budgets: None,
            tool_choice: None,
        };
        self.stream_simple(model, context, &simple_options)
    }

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
            let output = create_output(&model, Api::OpenAIResponses);
            match stream_responses(&model, &context, &options, output, &sender).await {
                Ok(()) => {}
                Err(error) => {
                    emit_error(&sender, create_output(&model, Api::OpenAIResponses), error)
                }
            }
        });
        stream
    }
}

/// Map a unified thinking effort to the Responses API reasoning effort string.
fn thinking_effort_to_effort(model: &Model, effort: &ThinkingEffort) -> Option<String> {
    if *effort == ThinkingEffort::Off {
        return None;
    }
    if let Some(mapped) = model
        .thinking_effort_map
        .as_ref()
        .and_then(|map| map.get(effort))
    {
        return mapped.clone();
    }
    use ThinkingEffort::*;
    Some(
        match effort {
            Off | Low => "low",
            Medium => "medium",
            High => "high",
            XHigh => "xhigh",
            Max => "max",
        }
        .to_string(),
    )
}

/// Core streaming logic: build request, send HTTP POST, parse SSE, normalize events.
async fn stream_responses(
    model: &Model,
    context: &Context,
    options: &SimpleStreamOptions,
    output: AssistantMessage,
    sender: &EventStreamSender<StreamEvent>,
) -> Result<(), ProviderError> {
    let _ = output; // output 由 normalizer 内部管理

    let api_key = resolve_api_key(model, &options.base)?;
    let client = build_http_client(&options.base)?;
    let base_url = &model.base_url;
    let url = join_url(base_url, "responses")?;

    // 构建请求 payload
    let system_prompt = context.system_prompt.as_deref();
    let input = convert_messages(&context.messages, system_prompt);
    let tools = if context.tools.is_empty() {
        None
    } else {
        Some(convert_tools(&context.tools))
    };

    // 从 options 构建 reasoning 配置
    let reasoning = options.reasoning.as_ref().and_then(|level| {
        thinking_effort_to_effort(model, level).map(|effort| Reasoning {
            effort: Some(effort),
            summary: Some("auto".to_string()),
        })
    });

    let request = ResponsesApiRequest {
        model: model.id.clone(),
        instructions: None,
        input,
        tools,
        tool_choice: "auto".to_string(),
        parallel_tool_calls: true,
        reasoning,
        store: false,
        stream: true,
        include: vec!["reasoning.encrypted_content".to_string()],
        service_tier: None,
        prompt_cache_key: options.base.session_id.clone(),
    };

    // 合并 headers
    let mut headers = merge_headers(model.headers.as_ref(), options.base.headers.as_ref());
    headers
        .entry("Authorization".to_string())
        .or_insert_with(|| format!("Bearer {api_key}"));
    headers
        .entry("Content-Type".to_string())
        .or_insert_with(|| "application/json".to_string());
    headers
        .entry("Accept".to_string())
        .or_insert_with(|| "text/event-stream".to_string());

    // 注入 ChatGPT-Account-ID（如通过 extra headers 传递）
    if let Some(account_id) = options
        .base
        .headers
        .as_ref()
        .and_then(|h| h.get("x-rozsa-account-id"))
    {
        headers.insert("ChatGPT-Account-ID".to_string(), account_id.clone());
    }

    // Session affinity headers
    if let Some(session_id) = &options.base.session_id {
        headers.insert("session_id".to_string(), session_id.clone());
        headers.insert("x-client-request-id".to_string(), session_id.clone());
    }

    let header_map = to_header_map(&headers)?;
    let payload = serde_json::to_value(&request)
        .map_err(|e| ProviderError::Parse(format!("Failed to serialize request: {e}")))?;

    // 发送 HTTP 请求
    let response = client
        .post(&url)
        .headers(header_map)
        .json(&payload)
        .send()
        .await
        .map_err(ProviderError::Http)?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(ProviderError::HttpStatus { status, body });
    }

    // 流式解析 SSE 事件
    let mut byte_stream = response.bytes_stream();
    let mut bytes_buffer = Vec::new();
    let mut parser = ResponsesSseParser::new();
    let mut normalizer = ResponseStreamNormalizer::new(model);

    while let Some(chunk) = byte_stream.next().await {
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
                    .map_err(|e| ProviderError::Parse(format!("stream not utf-8: {e}")))?
                    .to_string();
                bytes_buffer.drain(..valid_up_to);
                text
            }
            Err(error) => {
                return Err(ProviderError::Parse(format!("stream not utf-8: {error}")));
            }
        };

        for event in parser.feed(&text)? {
            for stream_event in normalizer.push_event(event) {
                sender.push(stream_event);
            }
        }
    }

    // 清理剩余缓冲
    if !bytes_buffer.is_empty() {
        let text = std::str::from_utf8(&bytes_buffer)
            .map_err(|e| ProviderError::Parse(format!("stream not utf-8: {e}")))?;
        for event in parser.feed(text)? {
            for stream_event in normalizer.push_event(event) {
                sender.push(stream_event);
            }
        }
    }
    for event in parser.finish()? {
        for stream_event in normalizer.push_event(event) {
            sender.push(stream_event);
        }
    }
    for stream_event in normalizer.finish() {
        sender.push(stream_event);
    }

    Ok(())
}
