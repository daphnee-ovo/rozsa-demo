use std::collections::BTreeMap;

use rozsa_app::tools::{
    ASK_USER_QUESTION_TOOL_NAME, AskUserQuestionAnswer, AskUserQuestionResponse,
    create_ask_user_question_tool,
};
use rozsa_core::tool::{ToolError, ToolExecutionMode};
use serde_json::json;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

fn params(multi_select: bool) -> serde_json::Value {
    json!({
        "questions": [{
            "question": "Which features should be enabled?",
            "header": "Features",
            "options": [
                {"label": "Lint", "description": "Run the linter."},
                {"label": "Tests", "description": "Run the test suite."}
            ],
            "multiSelect": multi_select
        }]
    })
}

#[tokio::test]
async fn exposes_the_forced_custom_option_contract_and_sequential_mode() {
    let (request_tx, _request_rx) = mpsc::unbounded_channel();
    let tool = create_ask_user_question_tool("session-a".to_string(), request_tx);

    assert_eq!(tool.name(), ASK_USER_QUESTION_TOOL_NAME);
    assert_eq!(tool.execution_mode(), Some(ToolExecutionMode::Sequential));
    let schema = tool.parameters_schema();
    assert!(schema["properties"]["questions"]["items"]["properties"]["multiSelect"].is_object());
    assert!(schema["properties"]["questions"]["items"]["properties"]["allowOther"].is_null());
}

#[tokio::test]
async fn returns_single_and_multi_answers_in_the_agent_result_shape() {
    let (request_tx, mut request_rx) = mpsc::unbounded_channel();
    let tool = create_ask_user_question_tool("session-a".to_string(), request_tx);

    let single = tokio::spawn({
        let tool = tool;
        async move { tool.execute("call-single", params(false), None, None).await }
    });
    let request = request_rx.recv().await.unwrap();
    assert_eq!(request.session_id, "session-a");
    assert_eq!(request.request_id, "call-single");
    let mut answers = BTreeMap::new();
    answers.insert(
        "Features".to_string(),
        AskUserQuestionAnswer::Single("my custom choice".to_string()),
    );
    request
        .response_tx
        .send(AskUserQuestionResponse::Answered { answers })
        .unwrap();
    let result = single.await.unwrap().unwrap();
    assert_eq!(
        result.details,
        json!({"answers": {"Features": "my custom choice"}})
    );

    let (request_tx, mut request_rx) = mpsc::unbounded_channel();
    let tool = create_ask_user_question_tool("session-a".to_string(), request_tx);
    let multi =
        tokio::spawn(async move { tool.execute("call-multi", params(true), None, None).await });
    let request = request_rx.recv().await.unwrap();
    let mut answers = BTreeMap::new();
    answers.insert(
        "Features".to_string(),
        AskUserQuestionAnswer::Multiple(vec![
            "Lint".to_string(),
            "another custom choice".to_string(),
        ]),
    );
    request
        .response_tx
        .send(AskUserQuestionResponse::Answered { answers })
        .unwrap();
    let result = multi.await.unwrap().unwrap();
    assert_eq!(
        result.details,
        json!({"answers": {"Features": ["Lint", "another custom choice"]}})
    );
}

#[tokio::test]
async fn invalid_payload_and_cancellation_are_explicit_errors() {
    let (request_tx, _request_rx) = mpsc::unbounded_channel();
    let tool = create_ask_user_question_tool("session-a".to_string(), request_tx);
    let error = tool
        .execute(
            "bad-call",
            json!({
                "questions": [{
                    "question": "Pick one",
                    "header": "Same",
                    "options": [{"label": "A", "description": "A"}],
                    "multiSelect": false
                }, {
                    "question": "Pick another",
                    "header": "Same",
                    "options": [{"label": "B", "description": "B"}],
                    "multiSelect": false
                }]
            }),
            None,
            None,
        )
        .await
        .unwrap_err();
    assert!(matches!(error, ToolError::Execution(message) if message.contains("unique")));

    let (request_tx, mut request_rx) = mpsc::unbounded_channel();
    let tool = create_ask_user_question_tool("session-a".to_string(), request_tx);
    let cancellation = CancellationToken::new();
    let task = tokio::spawn({
        let cancellation = cancellation.clone();
        async move {
            tool.execute("cancel-call", params(false), Some(cancellation), None)
                .await
        }
    });
    let _request = request_rx.recv().await.unwrap();
    cancellation.cancel();
    let error = task.await.unwrap().unwrap_err();
    assert!(matches!(error, ToolError::Cancelled));
}
