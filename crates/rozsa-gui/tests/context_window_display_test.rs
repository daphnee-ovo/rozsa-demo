// FrameworkTree
// context_window_display_test.rs
// ├── assistant_with_usage()
// ├── context_usage_uses_the_latest_prompt_not_cumulative_session_usage()
// └── settings_ui_uses_compaction_ratios()

use rozsa_core::messages::AgentMessage;
use rozsa_gui::state::context_usage_from_messages;
use rozsa_model::types::{Api, AssistantMessage, Provider, StopReason, Usage};

fn assistant_with_usage(input: u64, output: u64, cached: u64, total: u64) -> AgentMessage {
    AgentMessage::standard(rozsa_model::types::Message::Assistant(AssistantMessage {
        content: vec![],
        api: Api::OpenAICompletions,
        provider: Provider::OpenAI,
        model: "test".to_string(),
        response_model: None,
        response_id: None,
        usage: Usage {
            input,
            output,
            cache_read: cached,
            cache_write: 0,
            total_tokens: total,
            ..Usage::default()
        },
        stop_reason: StopReason::Stop,
        error_message: None,
        timestamp: 0,
    }))
}

#[test]
fn context_usage_uses_the_latest_prompt_not_cumulative_session_usage() {
    let messages = vec![
        assistant_with_usage(90_000, 1_000, 0, 91_000),
        assistant_with_usage(12_000, 800, 3_000, 15_800),
    ];

    let usage = context_usage_from_messages(&messages, 128_000);

    assert_eq!(usage.tokens, 15_000);
    assert_eq!(usage.input_tokens, 15_000);
    assert_eq!(usage.cached_input_tokens, 3_000);
    assert_eq!(usage.output_tokens, 800);
    assert!((usage.percent - (15_000.0 / 128_000.0 * 100.0)).abs() < f64::EPSILON);
}

#[test]
fn settings_ui_uses_compaction_ratios() {
    let commands = include_str!("../src/commands.rs");
    let app = include_str!("../frontend/app.js");
    let html = include_str!("../frontend/index.html");

    assert!(commands.contains("compaction_trigger_ratio"));
    assert!(commands.contains("compaction_target_ratio"));
    assert!(app.contains("settingsCompactionTriggerRatio"));
    assert!(app.contains("settingsCompactionTargetRatio"));
    assert!(html.contains("min=\"0\" max=\"1\" step=\"0.01\""));
}
