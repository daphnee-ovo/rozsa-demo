//! Anthropic Messages provider implementation.
//!
//! Internal Framework:
//! anthropic/mod.rs
//! ├── AnthropicProvider
//! │   ├── api()
//! │   ├── stream()
//! │   └── stream_simple()
//! └── stream_anthropic_response() — HTTP request + stream consumption
//!
//! Related Docs:
//! - [Supported Providers](../../../../docs/model/supported-providers.md)

pub mod payload;
pub mod stream;

use crate::env_keys::get_env_api_key;
use crate::event_stream::{EventStream, create_event_stream};
use crate::providers::common::{
    ProviderError, ProviderResult, build_http_client, create_output, emit_error, to_header_map,
};
use crate::registry::ApiProvider;
use crate::types::{
    Api, AssistantMessage, Context, Model, SimpleStreamOptions, StreamEvent, StreamOptions,
};

use self::payload::{
    build_anthropic_headers, build_messages_payload, is_oauth_token, resolve_compat,
};
use self::stream::consume_anthropic_stream;

pub struct AnthropicProvider {
    api: Api,
}

impl AnthropicProvider {
    pub fn new() -> Self {
        Self {
            api: Api::AnthropicMessages,
        }
    }
}

impl Default for AnthropicProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl ApiProvider for AnthropicProvider {
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
            let output = create_output(&model, Api::AnthropicMessages);
            match stream_anthropic_response(&model, &context, &options, output, &sender).await {
                Ok(()) => {}
                Err(error) => emit_error(
                    &sender,
                    create_output(&model, Api::AnthropicMessages),
                    error,
                ),
            }
        });
        stream
    }
}

async fn stream_anthropic_response(
    model: &Model,
    context: &Context,
    options: &SimpleStreamOptions,
    mut output: AssistantMessage,
    sender: &crate::event_stream::EventStreamSender<StreamEvent>,
) -> ProviderResult<()> {
    let api_key = resolve_api_key(model, &options.base)?;
    let is_oauth = is_oauth_token(&api_key);
    let compat = resolve_compat(model);

    let payload = build_messages_payload(model, context, options, is_oauth, &compat);
    let headers = build_anthropic_headers(model, options, is_oauth, &compat, context);

    let url = resolve_endpoint(model)?;
    let client = build_http_client(&options.base)?;

    let mut req_headers = to_header_map(&headers)?;
    // Auth header
    if is_oauth || is_copilot_provider(model) {
        req_headers.insert(
            reqwest::header::AUTHORIZATION,
            format!("Bearer {api_key}")
                .parse()
                .map_err(|_| ProviderError::Parse("invalid auth header value".to_string()))?,
        );
    } else if is_cloudflare_gateway(model) {
        req_headers.insert(
            reqwest::header::HeaderName::from_static("cf-aig-authorization"),
            format!("Bearer {api_key}").parse().map_err(|_| {
                ProviderError::Parse("invalid cf-aig-authorization header value".to_string())
            })?,
        );
    } else {
        req_headers.insert(
            reqwest::header::HeaderName::from_static("x-api-key"),
            api_key
                .parse()
                .map_err(|_| ProviderError::Parse("invalid x-api-key header value".to_string()))?,
        );
    }

    let response = client
        .post(&url)
        .headers(req_headers)
        .json(&payload)
        .send()
        .await
        .map_err(ProviderError::Http)?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(ProviderError::HttpStatus { status, body });
    }

    consume_anthropic_stream(
        response,
        model,
        &mut output,
        sender,
        is_oauth,
        &context.tools,
    )
    .await?;

    sender.push(StreamEvent::Done {
        reason: output.stop_reason,
        message: output,
    });

    Ok(())
}

fn resolve_api_key(model: &Model, options: &StreamOptions) -> ProviderResult<String> {
    if let Some(api_key) = options.api_key.as_ref().filter(|v| !v.is_empty()) {
        return Ok(api_key.clone());
    }
    get_env_api_key(&model.provider).ok_or_else(|| ProviderError::MissingApiKey {
        provider: crate::providers::common::provider_id(&model.provider),
    })
}

fn resolve_endpoint(model: &Model) -> ProviderResult<String> {
    let base = model.base_url.trim().trim_end_matches('/');
    if base.is_empty() {
        return Ok("https://api.anthropic.com/v1/messages".to_string());
    }
    // If the URL already ends with /messages, use as-is
    if base.ends_with("/messages") {
        return Ok(base.to_string());
    }
    // Append /v1/messages if no path segments suggest a versioned endpoint
    if base.contains("/v1") {
        Ok(format!("{base}/messages"))
    } else {
        Ok(format!("{base}/v1/messages"))
    }
}

fn is_copilot_provider(model: &Model) -> bool {
    matches!(&model.provider, crate::types::Provider::Custom(v) if v == "github-copilot")
}

fn is_cloudflare_gateway(model: &Model) -> bool {
    matches!(model.provider, crate::types::Provider::CloudflareAIGateway)
}
