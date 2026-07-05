// Converts between rozsa-model internal message types and the OpenAI Responses API
// input/output format (ResponseItems, ResponseEvents → StreamEvents).
//
// Internal Framework:
// convert.rs
// ├── convert_messages()             — Message[] + system → Vec<ResponseItem>
// ├── convert_tools()                — ToolSchema[] → Vec<Value> (function tools)
// ├── ResponseStreamNormalizer       — stateful SSE event → StreamEvent converter
// │   ├── new()
// │   ├── push_event()
// │   └── finish()
// └── content_to_string()            — Vec<ContentBlock> → plain text
//
// Reference:
// - pi/packages/ai/src/api/openai-responses-shared.ts (convertResponsesMessages)
// - codex-rs codex-api/src/sse/responses.rs (SSE event handling)
//
// Related Docs:
// - [Responses API types](./types.rs)

use std::collections::HashMap;

use serde_json::{Value, json};

use crate::providers::common::{calculate_cost, create_output};
use crate::types::{
    Api, AssistantMessage, ContentBlock, Message, Model, StopReason, StreamEvent, ToolCall,
    ToolSchema, Usage, UserContent,
};

use super::types::{ContentItem, ReasoningSummaryPart, ResponseEvent, ResponseItem, TokenUsage};

// ---------------------------------------------------------------------------
// convert_messages
// ---------------------------------------------------------------------------

/// Convert internal conversation messages and optional system prompt into
/// Responses API input items.
pub fn convert_messages(messages: &[Message], system_prompt: Option<&str>) -> Vec<ResponseItem> {
    let mut items: Vec<ResponseItem> = Vec::new();

    // System prompt → developer role message
    if let Some(prompt) = system_prompt.filter(|s| !s.is_empty()) {
        items.push(ResponseItem::Message {
            id: None,
            role: "developer".to_string(),
            content: vec![ContentItem::InputText {
                text: prompt.to_string(),
            }],
        });
    }

    for msg in messages {
        match msg {
            Message::User(user) => {
                let content = match &user.content {
                    UserContent::Text(text) => {
                        vec![ContentItem::InputText { text: text.clone() }]
                    }
                    UserContent::Blocks(blocks) => blocks
                        .iter()
                        .filter_map(block_to_input_content_item)
                        .collect(),
                };
                if content.is_empty() {
                    continue;
                }
                items.push(ResponseItem::Message {
                    id: None,
                    role: "user".to_string(),
                    content,
                });
            }
            Message::Assistant(assistant) => {
                convert_assistant_blocks(&assistant.content, &mut items);
            }
            Message::ToolResult(tool_result) => {
                let call_id = extract_call_id(&tool_result.tool_call_id);
                let output = content_to_string(&tool_result.content);
                items.push(ResponseItem::FunctionCallOutput {
                    id: None,
                    call_id,
                    output,
                });
            }
        }
    }

    items
}

/// Convert user-side content blocks to Responses API input content items.
fn block_to_input_content_item(block: &ContentBlock) -> Option<ContentItem> {
    match block {
        ContentBlock::Text { text, .. } => Some(ContentItem::InputText { text: text.clone() }),
        ContentBlock::Image { data, mime_type } => Some(ContentItem::InputImage {
            image_url: format!("data:{mime_type};base64,{data}"),
            detail: None,
        }),
        // Thinking / ToolCall 在 user blocks 中不转换
        _ => None,
    }
}

/// Expand assistant content blocks into individual ResponseItems.
/// Text → Message(assistant, OutputText), Thinking → Reasoning,
/// ToolCall → FunctionCall.
fn convert_assistant_blocks(blocks: &[ContentBlock], items: &mut Vec<ResponseItem>) {
    for block in blocks {
        match block {
            ContentBlock::Text { text, .. } => {
                items.push(ResponseItem::Message {
                    id: None,
                    role: "assistant".to_string(),
                    content: vec![ContentItem::OutputText { text: text.clone() }],
                });
            }
            ContentBlock::Thinking {
                thinking,
                signature,
                ..
            } => {
                let summary = if thinking.is_empty() {
                    Vec::new()
                } else {
                    vec![ReasoningSummaryPart {
                        text: thinking.clone(),
                    }]
                };
                items.push(ResponseItem::Reasoning {
                    id: None,
                    summary,
                    encrypted_content: signature.clone(),
                });
            }
            ContentBlock::ToolCall(tc) => {
                let call_id = extract_call_id(&tc.id);
                items.push(ResponseItem::FunctionCall {
                    id: None,
                    name: tc.name.clone(),
                    arguments: serde_json::to_string(&tc.arguments).unwrap_or_default(),
                    call_id,
                });
            }
            ContentBlock::Image { .. } => {
                // Images 不在 assistant 输出侧产生 ResponseItem
            }
        }
    }
}

/// Extract the call_id portion from a potentially compound id ("call_id|item_id").
fn extract_call_id(id: &str) -> String {
    id.split('|').next().unwrap_or(id).to_string()
}

/// Join text content blocks into a single string. Text blocks are joined with
/// newline; if the content contains only non-text blocks, returns a placeholder.
pub fn content_to_string(blocks: &[ContentBlock]) -> String {
    let texts: Vec<&str> = blocks
        .iter()
        .filter_map(|b| match b {
            ContentBlock::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();

    if texts.is_empty() {
        // 无文本内容时，尝试以 JSON 序列化整个内容（兼容混合结构）
        if blocks.is_empty() {
            String::new()
        } else {
            serde_json::to_string(blocks).unwrap_or_default()
        }
    } else {
        texts.join("\n")
    }
}

// ---------------------------------------------------------------------------
// convert_tools
// ---------------------------------------------------------------------------

/// Convert internal tool schemas to Responses API function tool definitions.
pub fn convert_tools(tools: &[ToolSchema]) -> Vec<Value> {
    tools
        .iter()
        .map(|tool| {
            json!({
                "type": "function",
                "name": tool.name,
                "description": tool.description,
                "parameters": tool.parameters,
                "strict": false,
            })
        })
        .collect()
}

// ---------------------------------------------------------------------------
// ResponseStreamNormalizer
// ---------------------------------------------------------------------------

/// Stateful normalizer that converts ResponseEvents from the Responses API SSE
/// stream into rozsa-model StreamEvent sequence. Accumulates output state
/// (text, thinking, tool calls) and emits appropriate start/delta/end events.
pub struct ResponseStreamNormalizer {
    output: AssistantMessage,
    model: Model,
    text_index: Option<usize>,
    thinking_index: Option<usize>,
    tool_call_indices: HashMap<String, usize>,
    tool_call_args: HashMap<String, String>,
    started: bool,
}

impl ResponseStreamNormalizer {
    /// Create a new normalizer pre-initialized with empty output for the model.
    pub fn new(model: &Model) -> Self {
        Self {
            output: create_output(model, Api::OpenAIResponses),
            model: model.clone(),
            text_index: None,
            thinking_index: None,
            tool_call_indices: HashMap::new(),
            tool_call_args: HashMap::new(),
            started: false,
        }
    }

    /// Process one ResponseEvent and return zero or more StreamEvents.
    pub fn push_event(&mut self, event: ResponseEvent) -> Vec<StreamEvent> {
        match event {
            ResponseEvent::Created => self.handle_created(),
            ResponseEvent::OutputTextDelta { delta } => self.handle_text_delta(delta),
            ResponseEvent::ReasoningSummaryDelta { delta, .. } => self.handle_thinking_delta(delta),
            ResponseEvent::ReasoningContentDelta { delta, .. } => self.handle_thinking_delta(delta),
            ResponseEvent::FunctionCallArgsDelta { call_id, delta, .. } => {
                self.handle_function_call_delta(call_id, delta)
            }
            ResponseEvent::OutputItemDone { item } => self.handle_output_item_done(item),
            ResponseEvent::Completed { response_id, usage } => {
                self.handle_completed(response_id, usage)
            }
            ResponseEvent::Failed { error_message, .. } => self.handle_failed(error_message),
            ResponseEvent::Incomplete { reason } => self.handle_incomplete(reason),
            ResponseEvent::OutputItemAdded { .. } => Vec::new(),
        }
    }

    /// Finalize and emit Done if not already emitted (safety fallback).
    pub fn finish(self) -> Vec<StreamEvent> {
        // 正常流程在 Completed/Failed 时已 emit Done，此处仅作安全兜底
        if !self.started {
            return Vec::new();
        }
        Vec::new()
    }

    // ── Event handlers ──────────────────────────────────────────────────────

    fn handle_created(&mut self) -> Vec<StreamEvent> {
        if self.started {
            return Vec::new();
        }
        self.started = true;
        vec![StreamEvent::Start {
            partial: self.output.clone(),
        }]
    }

    fn handle_text_delta(&mut self, delta: String) -> Vec<StreamEvent> {
        let mut events = Vec::new();

        // 确保 started
        if !self.started {
            events.extend(self.handle_created());
        }

        let idx = match self.text_index {
            Some(idx) => idx,
            None => {
                let idx = self.output.content.len();
                self.output.content.push(ContentBlock::Text {
                    text: String::new(),
                    signature: None,
                });
                self.text_index = Some(idx);
                events.push(StreamEvent::TextStart {
                    content_index: idx,
                    partial: self.output.clone(),
                });
                idx
            }
        };

        // 追加 delta 文本
        if let Some(ContentBlock::Text { text, .. }) = self.output.content.get_mut(idx) {
            text.push_str(&delta);
        }

        events.push(StreamEvent::TextDelta {
            content_index: idx,
            delta,
            partial: self.output.clone(),
        });

        events
    }

    fn handle_thinking_delta(&mut self, delta: String) -> Vec<StreamEvent> {
        let mut events = Vec::new();

        if !self.started {
            events.extend(self.handle_created());
        }

        let idx = match self.thinking_index {
            Some(idx) => idx,
            None => {
                let idx = self.output.content.len();
                self.output.content.push(ContentBlock::Thinking {
                    thinking: String::new(),
                    signature: None,
                    redacted: false,
                });
                self.thinking_index = Some(idx);
                events.push(StreamEvent::ThinkingStart {
                    content_index: idx,
                    partial: self.output.clone(),
                });
                idx
            }
        };

        if let Some(ContentBlock::Thinking { thinking, .. }) = self.output.content.get_mut(idx) {
            thinking.push_str(&delta);
        }

        events.push(StreamEvent::ThinkingDelta {
            content_index: idx,
            delta,
            partial: self.output.clone(),
        });

        events
    }

    fn handle_function_call_delta(
        &mut self,
        call_id: Option<String>,
        delta: String,
    ) -> Vec<StreamEvent> {
        let mut events = Vec::new();

        if !self.started {
            events.extend(self.handle_created());
        }

        let key = call_id.unwrap_or_default();

        let idx = if let Some(&idx) = self.tool_call_indices.get(&key) {
            idx
        } else {
            let idx = self.output.content.len();
            self.output.content.push(ContentBlock::ToolCall(ToolCall {
                id: key.clone(),
                name: String::new(),
                arguments: Value::Null,
            }));
            self.tool_call_indices.insert(key.clone(), idx);
            self.tool_call_args.insert(key.clone(), String::new());
            events.push(StreamEvent::ToolCallStart {
                content_index: idx,
                partial: self.output.clone(),
            });
            idx
        };

        // 累积 args 字符串
        if let Some(accumulated) = self.tool_call_args.get_mut(&key) {
            accumulated.push_str(&delta);
        }

        events.push(StreamEvent::ToolCallDelta {
            content_index: idx,
            delta,
            partial: self.output.clone(),
        });

        events
    }

    fn handle_output_item_done(&mut self, item: ResponseItem) -> Vec<StreamEvent> {
        let mut events = Vec::new();

        match item {
            ResponseItem::Message { content, .. } => {
                // OutputText done — finalize text block
                for content_item in &content {
                    if let ContentItem::OutputText { text } = content_item
                        && let Some(idx) = self.text_index
                    {
                        // 用最终文本替换累积结果
                        if let Some(ContentBlock::Text { text: t, .. }) =
                            self.output.content.get_mut(idx)
                        {
                            *t = text.clone();
                        }
                        events.push(StreamEvent::TextEnd {
                            content_index: idx,
                            content: text.clone(),
                            partial: self.output.clone(),
                        });
                        // 重置以支持多个文本块
                        self.text_index = None;
                    }
                }
            }
            ResponseItem::FunctionCall {
                name,
                arguments,
                call_id,
                ..
            } => {
                let key = call_id.clone();
                if let Some(&idx) = self.tool_call_indices.get(&key) {
                    let parsed_args: Value =
                        serde_json::from_str(&arguments).unwrap_or(Value::Null);
                    let tool_call = ToolCall {
                        id: call_id,
                        name: name.clone(),
                        arguments: parsed_args.clone(),
                    };

                    // 更新 output 中的 ToolCall 块
                    if let Some(block) = self.output.content.get_mut(idx) {
                        *block = ContentBlock::ToolCall(tool_call.clone());
                    }

                    events.push(StreamEvent::ToolCallEnd {
                        content_index: idx,
                        tool_call,
                        partial: self.output.clone(),
                    });
                } else {
                    // call_id 未见过（理论上不应发生），直接追加
                    let idx = self.output.content.len();
                    let parsed_args: Value =
                        serde_json::from_str(&arguments).unwrap_or(Value::Null);
                    let tool_call = ToolCall {
                        id: call_id,
                        name,
                        arguments: parsed_args,
                    };
                    self.output
                        .content
                        .push(ContentBlock::ToolCall(tool_call.clone()));
                    events.push(StreamEvent::ToolCallEnd {
                        content_index: idx,
                        tool_call,
                        partial: self.output.clone(),
                    });
                }
            }
            ResponseItem::Reasoning {
                encrypted_content,
                summary,
                ..
            } => {
                if let Some(idx) = self.thinking_index {
                    // 从 summary 提取最终文本
                    let final_text = summary.iter().map(|p| p.text.as_str()).collect::<String>();

                    if let Some(ContentBlock::Thinking {
                        thinking,
                        signature,
                        ..
                    }) = self.output.content.get_mut(idx)
                    {
                        if !final_text.is_empty() {
                            *thinking = final_text.clone();
                        }
                        *signature = encrypted_content;
                    }

                    let content = if final_text.is_empty() {
                        if let Some(ContentBlock::Thinking { thinking, .. }) =
                            self.output.content.get(idx)
                        {
                            thinking.clone()
                        } else {
                            String::new()
                        }
                    } else {
                        final_text
                    };

                    events.push(StreamEvent::ThinkingEnd {
                        content_index: idx,
                        content,
                        partial: self.output.clone(),
                    });
                    // 重置以支持多个 reasoning 块
                    self.thinking_index = None;
                }
            }
            ResponseItem::FunctionCallOutput { .. } => {
                // 输出侧不应出现 FunctionCallOutput
            }
        }

        events
    }

    fn handle_completed(
        &mut self,
        response_id: String,
        usage: Option<TokenUsage>,
    ) -> Vec<StreamEvent> {
        self.output.response_id = Some(response_id);
        self.output.stop_reason = StopReason::Stop;

        if let Some(token_usage) = usage {
            self.output.usage = convert_token_usage(&token_usage);
            calculate_cost(&self.model, &mut self.output.usage);
        }

        // 如果有未关闭的工具调用，判断 stop reason
        if self
            .output
            .content
            .iter()
            .any(|b| matches!(b, ContentBlock::ToolCall(_)))
        {
            self.output.stop_reason = StopReason::ToolUse;
        }

        vec![StreamEvent::Done {
            reason: self.output.stop_reason,
            message: self.output.clone(),
        }]
    }

    fn handle_failed(&mut self, error_message: String) -> Vec<StreamEvent> {
        self.output.stop_reason = StopReason::Error;
        self.output.error_message = Some(error_message);
        vec![StreamEvent::Error {
            reason: StopReason::Error,
            error: self.output.clone(),
        }]
    }

    fn handle_incomplete(&mut self, reason: Option<String>) -> Vec<StreamEvent> {
        self.output.stop_reason = StopReason::Length;
        if let Some(r) = reason {
            self.output.error_message = Some(r);
        }
        vec![StreamEvent::Done {
            reason: StopReason::Length,
            message: self.output.clone(),
        }]
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Convert Responses API token usage to internal Usage struct.
fn convert_token_usage(tu: &TokenUsage) -> Usage {
    Usage {
        input: tu.input_tokens,
        output: tu.output_tokens,
        cache_read: tu.cached_tokens.unwrap_or(0),
        cache_write: 0,
        total_tokens: tu.total_tokens,
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Provider;

    fn assistant_with_thinking(thinking: &str, signature: Option<&str>) -> Message {
        Message::Assistant(AssistantMessage {
            content: vec![ContentBlock::Thinking {
                thinking: thinking.to_string(),
                signature: signature.map(str::to_string),
                redacted: false,
            }],
            api: Api::OpenAIResponses,
            provider: Provider::Custom("codex-oauth".to_string()),
            model: "gpt-5.4".to_string(),
            response_model: None,
            response_id: Some("resp-1".to_string()),
            usage: Usage::default(),
            stop_reason: StopReason::Stop,
            error_message: None,
            timestamp: 0,
        })
    }

    #[test]
    fn assistant_thinking_replay_includes_required_summary() {
        let items = convert_messages(
            &[assistant_with_thinking("reasoning summary", Some("enc"))],
            None,
        );

        let value = serde_json::to_value(&items[0]).expect("reasoning item should serialize");
        assert_eq!(value["type"], "reasoning");
        assert_eq!(value["summary"][0]["text"], "reasoning summary");
        assert_eq!(value["encrypted_content"], "enc");
    }

    #[test]
    fn empty_assistant_thinking_replay_serializes_empty_summary() {
        let items = convert_messages(&[assistant_with_thinking("", Some("enc"))], None);

        let value = serde_json::to_value(&items[0]).expect("reasoning item should serialize");
        assert_eq!(value["type"], "reasoning");
        assert_eq!(value["summary"], serde_json::json!([]));
        assert_eq!(value["encrypted_content"], "enc");
    }
}
