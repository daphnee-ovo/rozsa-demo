// apply_event 累积逻辑单测 —— 钉住「消息不重复」的契约。
//
// 背景：agent_loop 对一轮对话会发出
//   AgentStart
//   MessageStart{user} / MessageEnd{user}
//   MessageStart{assistant} / MessageUpdate.. / MessageEnd{assistant}
//   AgentEnd{messages:[user, assistant, ...]}   ← 本轮全量（含 tool result）
//
// MessageStart 已经把 user/assistant 增量累积进 live.messages，AgentEnd 携带的
// 是同一批消息的权威全量。早期实现里 AgentEnd 直接 append，导致每条消息翻倍
// （UI 上「输入一句显示两句」）。apply_event 现在用 turn_base 把本轮 truncate
// 后再 extend，保证最终列表 == AgentEnd 的权威列表，无重复。

use rozsa_core::events::AgentEvent;
use rozsa_core::messages::AgentMessage;
use rozsa_model::types::{
    Api, AssistantMessage, ContentBlock, Message, Provider, StopReason, ToolResultMessage, Usage,
    UsageCost, UserContent, UserMessage,
};
use rozsa_tui::backend::native::{apply_event, LiveState};

fn user(text: &str) -> AgentMessage {
    AgentMessage::standard(Message::User(UserMessage {
        content: UserContent::Text(text.to_string()),
        display_text: None,
        timestamp: 1,
    }))
}

fn assistant(text: &str) -> AgentMessage {
    AgentMessage::standard(Message::Assistant(AssistantMessage {
        content: vec![ContentBlock::Text {
            text: text.to_string(),
            signature: None,
        }],
        api: Api::OpenAIResponses,
        provider: Provider::OpenAI,
        model: "test".to_string(),
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
        stop_reason: StopReason::Stop,
        error_message: None,
        timestamp: 2,
    }))
}

fn tool_result(id: &str) -> AgentMessage {
    AgentMessage::standard(Message::ToolResult(ToolResultMessage {
        tool_call_id: id.to_string(),
        tool_name: "bash".to_string(),
        content: vec![ContentBlock::Text {
            text: "ok".to_string(),
            signature: None,
        }],
        is_error: false,
        timestamp: 3,
    }))
}

fn text_of(msg: &AgentMessage) -> String {
    match msg.as_standard() {
        Some(Message::User(u)) => match &u.content {
            UserContent::Text(t) => t.clone(),
            _ => "<blocks>".to_string(),
        },
        Some(Message::Assistant(a)) => a
            .content
            .iter()
            .filter_map(|b| match b {
                ContentBlock::Text { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect(),
        Some(Message::ToolResult(tr)) => format!("toolresult:{}", tr.tool_call_id),
        None => "<custom>".to_string(),
    }
}

/// 一轮简单对话（无工具）：最终应只有 user + assistant 各一条，不翻倍。
#[test]
fn simple_turn_does_not_duplicate_messages() {
    let mut live = LiveState::empty();
    let u = user("1+1=?");
    let a = assistant("2");

    apply_event(&mut live, &AgentEvent::AgentStart);
    apply_event(&mut live, &AgentEvent::MessageStart { message: u.clone() });
    apply_event(&mut live, &AgentEvent::MessageEnd { message: u.clone() });
    apply_event(&mut live, &AgentEvent::MessageStart { message: a.clone() });
    apply_event(&mut live, &AgentEvent::MessageEnd { message: a.clone() });
    apply_event(
        &mut live,
        &AgentEvent::AgentEnd {
            messages: vec![u.clone(), a.clone()],
        },
    );

    assert_eq!(live.messages.len(), 2, "expected exactly user + assistant");
    assert_eq!(text_of(&live.messages[0]), "1+1=?");
    assert_eq!(text_of(&live.messages[1]), "2");
    assert!(!live.is_streaming);
}

/// AgentEnd 携带的 tool-result（从未发过 MessageStart）必须出现在最终列表里。
#[test]
fn agent_end_supplies_tool_result_messages() {
    let mut live = LiveState::empty();
    let u = user("run ls");
    let a = assistant("calling bash");
    let tr = tool_result("call_1");

    apply_event(&mut live, &AgentEvent::AgentStart);
    apply_event(&mut live, &AgentEvent::MessageStart { message: u.clone() });
    apply_event(&mut live, &AgentEvent::MessageEnd { message: u.clone() });
    apply_event(&mut live, &AgentEvent::MessageStart { message: a.clone() });
    apply_event(&mut live, &AgentEvent::MessageEnd { message: a.clone() });
    // tool result 不发 MessageStart，只在 AgentEnd 的全量里出现。
    apply_event(
        &mut live,
        &AgentEvent::AgentEnd {
            messages: vec![u, a, tr],
        },
    );

    assert_eq!(live.messages.len(), 3);
    assert_eq!(text_of(&live.messages[2]), "toolresult:call_1");
}

/// 多轮对话：前一轮的消息应保留，第二轮也不翻倍。
#[test]
fn second_turn_preserves_history_and_does_not_duplicate() {
    let mut live = LiveState::empty();
    let u1 = user("hello");
    let a1 = assistant("hi");

    apply_event(&mut live, &AgentEvent::AgentStart);
    apply_event(&mut live, &AgentEvent::MessageStart { message: u1.clone() });
    apply_event(&mut live, &AgentEvent::MessageStart { message: a1.clone() });
    apply_event(
        &mut live,
        &AgentEvent::AgentEnd {
            messages: vec![u1.clone(), a1.clone()],
        },
    );
    assert_eq!(live.messages.len(), 2);

    let u2 = user("bye");
    let a2 = assistant("goodbye");
    apply_event(&mut live, &AgentEvent::AgentStart);
    apply_event(&mut live, &AgentEvent::MessageStart { message: u2.clone() });
    apply_event(&mut live, &AgentEvent::MessageStart { message: a2.clone() });
    apply_event(
        &mut live,
        &AgentEvent::AgentEnd {
            messages: vec![u2, a2],
        },
    );

    assert_eq!(live.messages.len(), 4, "two turns => 4 messages, no doubling");
    assert_eq!(text_of(&live.messages[0]), "hello");
    assert_eq!(text_of(&live.messages[1]), "hi");
    assert_eq!(text_of(&live.messages[2]), "bye");
    assert_eq!(text_of(&live.messages[3]), "goodbye");
}
