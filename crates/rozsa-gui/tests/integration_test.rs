// File: integration_test.rs
//
// rozsa-gui 集成测试 — 验证 LiveState 事件累积和 UiSnapshot 序列化格式。
// 作为独立 QA 验证，不依赖开发期间的手工检查。

use rozsa_core::events::AgentEvent;
use rozsa_core::messages::AgentMessage;
use rozsa_gui::state::LiveState;
use rozsa_model::types::*;
use serde_json::Value;

/// 构造一个标准的 user message
fn make_user_message(text: &str, timestamp: i64) -> AgentMessage {
    AgentMessage::standard(Message::User(UserMessage {
        content: UserContent::Text(text.to_string()),
        display_text: None,
        timestamp,
    }))
}

/// 构造一个 assistant message with thinking + text + tool call
fn make_assistant_message(timestamp: i64) -> AgentMessage {
    AgentMessage::standard(Message::Assistant(AssistantMessage {
        content: vec![
            ContentBlock::Thinking {
                thinking: "analyzing request".to_string(),
                signature: None,
                redacted: false,
            },
            ContentBlock::Text {
                text: "I'll run a bash command".to_string(),
                signature: None,
            },
            ContentBlock::ToolCall(ToolCall {
                id: "tc_001".to_string(),
                name: "Bash".to_string(),
                arguments: serde_json::json!({"command": "ls -la"}),
            }),
        ],
        api: Api::AnthropicMessages,
        provider: Provider::Anthropic,
        model: "claude-opus-4".to_string(),
        response_model: None,
        response_id: Some("resp_001".to_string()),
        usage: Usage {
            input: 150,
            output: 80,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 230,
            cost: UsageCost::default(),
        },
        stop_reason: StopReason::ToolUse,
        error_message: None,
        timestamp,
    }))
}

/// 构造一个 tool result message
fn make_tool_result_message(tool_call_id: &str, output: &str, timestamp: i64) -> AgentMessage {
    AgentMessage::standard(Message::ToolResult(ToolResultMessage {
        tool_call_id: tool_call_id.to_string(),
        tool_name: "Bash".to_string(),
        content: vec![ContentBlock::Text {
            text: output.to_string(),
            signature: None,
        }],
        details: Value::Null,
        is_error: false,
        timestamp,
    }))
}

#[test]
fn test_live_state_event_accumulation() {
    // Test case 1: 验证 AgentEvent 序列的累积逻辑
    let mut state = LiveState::default();

    // AgentStart
    let changed = state.apply(&AgentEvent::AgentStart);
    assert!(changed, "AgentStart should mark changed");
    assert!(state.is_streaming, "Should be streaming after AgentStart");
    assert_eq!(state.messages.len(), 0, "No messages yet");

    // MessageStart - user message
    let user_msg = make_user_message("hello", 1000);
    let changed = state.apply(&AgentEvent::MessageStart {
        message: user_msg.clone(),
    });
    assert!(changed, "MessageStart should mark changed");
    assert_eq!(state.messages.len(), 1, "Should have 1 message");
    if let AgentMessage::Standard {
        message: Message::User(u),
    } = &state.messages[0]
    {
        assert_eq!(u.timestamp, 1000, "Timestamp should match");
    } else {
        panic!("Expected user message");
    }

    // MessageStart - assistant message
    let assistant_msg = make_assistant_message(2000);
    let changed = state.apply(&AgentEvent::MessageStart {
        message: assistant_msg.clone(),
    });
    assert!(changed);
    assert_eq!(state.messages.len(), 2, "Should have 2 messages");

    // MessageUpdate - simulate streaming update
    let mut updated_assistant = make_assistant_message(2000);
    if let AgentMessage::Standard {
        message: Message::Assistant(ref mut a),
    } = updated_assistant
    {
        a.content.push(ContentBlock::Text {
            text: " (updated)".to_string(),
            signature: None,
        });
    }
    let changed = state.apply(&AgentEvent::MessageUpdate {
        message: updated_assistant.clone(),
        stream_event: rozsa_model::types::StreamEvent::Start {
            partial: AssistantMessage {
                content: vec![],
                api: Api::AnthropicMessages,
                provider: Provider::Anthropic,
                model: "test".to_string(),
                response_model: None,
                response_id: None,
                usage: Usage::default(),
                stop_reason: StopReason::Stop,
                error_message: None,
                timestamp: 2000,
            },
        },
    });
    assert!(changed);
    assert_eq!(state.messages.len(), 2, "Should still have 2 messages");
    // Last message should be updated
    if let AgentMessage::Standard {
        message: Message::Assistant(a),
    } = &state.messages[1]
    {
        assert_eq!(a.content.len(), 4, "Should have 4 content blocks after update");
    } else {
        panic!("Expected assistant message");
    }

    // ToolExecutionStart
    let changed = state.apply(&AgentEvent::ToolExecutionStart {
        tool_call_id: "tc_001".to_string(),
        tool_name: "Bash".to_string(),
        args: serde_json::json!({"command": "ls"}),
    });
    assert!(changed, "ToolExecutionStart should mark changed");

    // ToolExecutionEnd
    let tool_result = make_tool_result_message("tc_001", "file1.txt\nfile2.txt", 3000);
    if let AgentMessage::Standard {
        message: Message::ToolResult(tr),
    } = &tool_result
    {
        let changed = state.apply(&AgentEvent::ToolExecutionEnd {
            tool_call_id: tr.tool_call_id.clone(),
            tool_name: tr.tool_name.clone(),
            result: tr.clone(),
        });
        assert!(changed);
    }

    // MessageStart - tool result message
    let changed = state.apply(&AgentEvent::MessageStart {
        message: tool_result.clone(),
    });
    assert!(changed);
    assert_eq!(state.messages.len(), 3, "Should have 3 messages now");

    // AgentEnd - 替换当前 turn 的所有消息
    let final_messages = vec![
        user_msg.clone(),
        make_assistant_message(2000),
        tool_result.clone(),
    ];
    let changed = state.apply(&AgentEvent::AgentEnd {
        messages: final_messages.clone(),
    });
    assert!(changed);
    assert!(!state.is_streaming, "Should not be streaming after AgentEnd");
    assert_eq!(
        state.messages.len(),
        3,
        "Should have exactly 3 messages after AgentEnd"
    );
}

#[test]
fn test_live_state_turn_replacement() {
    // Test case 2: 验证 AgentEnd 的 truncate + extend 逻辑
    let mut state = LiveState::default();

    // 初始状态：已有一些历史消息
    let history_msg = make_user_message("history", 500);
    state.messages.push(history_msg.clone());
    assert_eq!(state.messages.len(), 1);

    // AgentStart - 记录 turn_base
    state.apply(&AgentEvent::AgentStart);
    assert_eq!(state.turn_base, 1, "turn_base should be 1");

    // 累积当前 turn 的消息
    let user_msg = make_user_message("current turn", 1000);
    state.apply(&AgentEvent::MessageStart {
        message: user_msg.clone(),
    });
    state.apply(&AgentEvent::MessageStart {
        message: make_assistant_message(2000),
    });
    assert_eq!(state.messages.len(), 3, "Should have history + 2 current messages");

    // AgentEnd - 应该 truncate 到 turn_base，然后 extend 新的权威列表
    let final_messages = vec![
        make_user_message("final user", 1000),
        make_assistant_message(2000),
    ];
    state.apply(&AgentEvent::AgentEnd {
        messages: final_messages.clone(),
    });

    assert_eq!(
        state.messages.len(),
        3,
        "Should have 1 history + 2 final messages"
    );
    // 第一条应该是历史消息
    if let AgentMessage::Standard {
        message: Message::User(u),
    } = &state.messages[0]
    {
        assert_eq!(u.content.text(), "history");
    } else {
        panic!("Expected history message");
    }
    // 后两条应该是 final_messages
    if let AgentMessage::Standard {
        message: Message::User(u),
    } = &state.messages[1]
    {
        assert_eq!(u.content.text(), "final user");
    } else {
        panic!("Expected final user message");
    }
}

#[test]
fn test_ui_snapshot_serialization_format() {
    // Test case 3: 验证 UiSnapshot 序列化后的 JSON 格式
    let user_msg = make_user_message("test user message", 1000);
    let assistant_msg = make_assistant_message(2000);
    let tool_result = make_tool_result_message("tc_001", "output text", 3000);

    // 序列化每个消息，检查 JSON 结构
    let user_json = serde_json::to_value(&user_msg).expect("Failed to serialize user message");
    eprintln!("USER JSON:\n{}\n", serde_json::to_string_pretty(&user_json).unwrap());

    assert_eq!(user_json["kind"], "standard", "kind should be 'standard'");
    assert!(user_json["message"]["content"].is_array(), "content should be array");
    assert_eq!(user_json["message"]["role"], "user", "role should be 'user'");
    assert_eq!(user_json["message"]["timestamp"], 1000, "timestamp should match");
    let user_content = user_json["message"]["content"].as_array().unwrap();
    assert_eq!(user_content.len(), 1, "User content should have 1 block");
    assert_eq!(user_content[0]["type"], "text", "First block should be text");
    assert_eq!(user_content[0]["text"], "test user message");

    let assistant_json = serde_json::to_value(&assistant_msg).expect("Failed to serialize assistant message");
    eprintln!("ASSISTANT JSON:\n{}\n", serde_json::to_string_pretty(&assistant_json).unwrap());

    assert_eq!(assistant_json["kind"], "standard");
    assert!(assistant_json["message"]["content"].is_array(), "content should be array");
    let content_arr = assistant_json["message"]["content"].as_array().unwrap();
    assert!(content_arr.len() >= 3, "Should have thinking + text + tool call");

    // 验证 thinking block
    assert_eq!(content_arr[0]["type"], "thinking", "First block should be thinking");
    assert_eq!(content_arr[0]["thinking"], "analyzing request");

    // 验证 text block
    assert_eq!(content_arr[1]["type"], "text", "Second block should be text");
    assert_eq!(content_arr[1]["text"], "I'll run a bash command");

    // 验证 tool call block
    assert_eq!(content_arr[2]["type"], "toolCall", "Third block should be toolCall");
    assert_eq!(content_arr[2]["id"], "tc_001");
    assert_eq!(content_arr[2]["name"], "Bash");
    assert!(content_arr[2]["arguments"].is_object(), "arguments should be object");

    let tool_result_json = serde_json::to_value(&tool_result).expect("Failed to serialize tool result");
    eprintln!("TOOL_RESULT JSON:\n{}\n", serde_json::to_string_pretty(&tool_result_json).unwrap());

    assert_eq!(tool_result_json["kind"], "standard");
    assert_eq!(tool_result_json["message"]["toolCallId"], "tc_001", "toolCallId should use camelCase");
    assert_eq!(tool_result_json["message"]["toolName"], "Bash", "toolName should use camelCase");
    assert_eq!(tool_result_json["message"]["isError"], false, "isError should use camelCase");
    assert!(tool_result_json["message"]["content"].is_array(), "content should be array");
}

#[test]
fn test_custom_message_format() {
    // Test case 4: 验证 custom message 的序列化
    let custom_msg = AgentMessage::custom(
        "status_update".to_string(),
        serde_json::json!({"status": "processing", "progress": 50}),
        4000,
    );

    let custom_json = serde_json::to_value(&custom_msg).expect("Failed to serialize custom message");
    eprintln!("CUSTOM JSON:\n{}\n", serde_json::to_string_pretty(&custom_json).unwrap());

    assert_eq!(custom_json["kind"], "custom", "kind should be 'custom'");
    assert_eq!(custom_json["message"]["message_type"], "status_update", "message_type should match");
    assert_eq!(custom_json["message"]["payload"]["status"], "processing");
    assert_eq!(custom_json["message"]["payload"]["progress"], 50);
    assert_eq!(custom_json["message"]["timestamp"], 4000);
}

#[test]
fn test_messages_array_in_ui_snapshot() {
    // Test case 5: 验证 messages 数组的完整序列化（模拟 UiSnapshot.messages 字段）
    let mut state = LiveState::default();

    state.apply(&AgentEvent::AgentStart);
    state.apply(&AgentEvent::MessageStart {
        message: make_user_message("hello", 1000),
    });
    state.apply(&AgentEvent::MessageStart {
        message: make_assistant_message(2000),
    });
    state.apply(&AgentEvent::MessageStart {
        message: make_tool_result_message("tc_001", "output", 3000),
    });

    // 模拟 UiSnapshot::from_state 中的序列化逻辑
    let messages_json: Vec<Value> = state
        .messages
        .iter()
        .filter_map(|m| serde_json::to_value(m).ok())
        .collect();

    assert_eq!(messages_json.len(), 3, "Should have 3 messages");

    // 验证每条消息的 kind 字段
    assert_eq!(messages_json[0]["kind"], "standard");
    assert_eq!(messages_json[1]["kind"], "standard");
    assert_eq!(messages_json[2]["kind"], "standard");

    // 验证消息的 role（通过检查 message 字段的类型）
    // user message 有 content 数组
    assert!(messages_json[0]["message"]["content"].is_array(), "User content should be array");
    assert_eq!(messages_json[0]["message"]["role"], "user", "First message should be user");
    assert!(messages_json[0]["message"]["model"].is_null(), "User message should not have model field");

    // assistant message 有 model 和 provider
    assert_eq!(messages_json[1]["message"]["model"], "claude-opus-4");
    assert_eq!(messages_json[1]["message"]["provider"], "anthropic");

    // tool result 有 toolCallId
    assert_eq!(messages_json[2]["message"]["toolCallId"], "tc_001");
}
