//! AWS Bedrock Converse Stream provider implementation.
//!
//! Internal Framework:
//! bedrock/mod.rs
//! ├── BedrockProvider
//! │   ├── api()
//! │   ├── stream()
//! │   └── stream_simple()
//! ├── build_bedrock_client()    — credential + region 构建
//! └── stream_bedrock_response() — 请求发送 + stream 消费入口
//!
//! Related Docs:
//! - [Supported Providers](../../../../docs/model/supported-providers.md)

pub mod payload;
pub mod stream;

use crate::event_stream::{EventStream, create_event_stream};
use crate::providers::common::{create_output, emit_error};
use crate::registry::ApiProvider;
use crate::types::{Api, Context, Model, SimpleStreamOptions, StreamEvent, StreamOptions};

use aws_config::BehaviorVersion;
use aws_sdk_bedrockruntime::Client as BedrockClient;
use aws_sdk_bedrockruntime::config::Builder as BedrockConfigBuilder;

pub struct BedrockProvider {
    api: Api,
}

impl BedrockProvider {
    pub fn new() -> Self {
        Self {
            api: Api::BedrockConverseStream,
        }
    }
}

impl Default for BedrockProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl ApiProvider for BedrockProvider {
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
            thinking_budgets: None,
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
        let (sender, event_stream) = create_event_stream();
        let model = model.clone();
        let context = context.clone();
        let options = options.clone();
        tokio::spawn(async move {
            let output = create_output(&model, Api::BedrockConverseStream);
            match stream_bedrock_response(&model, &context, &options, output, &sender).await {
                Ok(()) => {}
                Err(error) => emit_error(
                    &sender,
                    create_output(&model, Api::BedrockConverseStream),
                    error,
                ),
            }
        });
        event_stream
    }
}

async fn build_bedrock_client() -> BedrockClient {
    let skip_auth = std::env::var("AWS_BEDROCK_SKIP_AUTH")
        .ok()
        .is_some_and(|v| v == "1");

    let bearer_token = std::env::var("AWS_BEARER_TOKEN_BEDROCK").ok();

    let sdk_config = aws_config::defaults(BehaviorVersion::latest()).load().await;

    let mut config_builder = BedrockConfigBuilder::from(&sdk_config);

    // Region fallback: if no region resolved, default to us-east-1.
    if sdk_config.region().is_none() {
        config_builder =
            config_builder.region(aws_sdk_bedrockruntime::config::Region::new("us-east-1"));
    }

    if skip_auth {
        let dummy_creds = aws_sdk_bedrockruntime::config::Credentials::new(
            "dummy-access-key",
            "dummy-secret-key",
            None,
            None,
            "skip-auth",
        );
        config_builder = config_builder.credentials_provider(dummy_creds);
    } else if let Some(token) = bearer_token {
        let token_provider = aws_sdk_bedrockruntime::config::Token::new(token, None);
        config_builder = config_builder.token_provider(token_provider);
    }

    BedrockClient::from_conf(config_builder.build())
}

async fn stream_bedrock_response(
    model: &Model,
    context: &Context,
    options: &SimpleStreamOptions,
    output: crate::types::AssistantMessage,
    sender: &crate::event_stream::EventStreamSender<StreamEvent>,
) -> Result<(), crate::providers::common::ProviderError> {
    let client = build_bedrock_client().await;

    let command_input = payload::build_converse_stream_input(model, context, options)?;

    let response = client
        .converse_stream()
        .model_id(&model.id)
        .set_messages(Some(command_input.messages))
        .set_system(command_input.system)
        .set_inference_config(command_input.inference_config)
        .set_tool_config(command_input.tool_config)
        .set_additional_model_request_fields(command_input.additional_model_request_fields)
        .send()
        .await
        .map_err(|e| crate::providers::common::ProviderError::Parse(format!("{e}")))?;

    stream::consume_stream(response.stream, model, output, sender).await;

    Ok(())
}
