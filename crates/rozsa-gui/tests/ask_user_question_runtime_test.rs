use std::collections::BTreeMap;
use std::sync::Arc;

use dashmap::DashMap;
use rozsa_app::tools::{
    AskUserQuestion, AskUserQuestionAnswer, AskUserQuestionOption, AskUserQuestionResponse,
};
use rozsa_gui::state::{
    PendingUserQuestion, cancel_pending_user_questions, respond_pending_user_question,
    user_question_pending_key,
};
use tokio::sync::oneshot;

fn question(multi_select: bool) -> AskUserQuestion {
    AskUserQuestion {
        question: "Which features should be enabled?".to_string(),
        header: "Features".to_string(),
        options: vec![
            AskUserQuestionOption {
                label: "Lint".to_string(),
                description: "Run the linter.".to_string(),
            },
            AskUserQuestionOption {
                label: "Tests".to_string(),
                description: "Run the test suite.".to_string(),
            },
        ],
        multi_select,
    }
}

#[tokio::test]
async fn response_is_validated_and_consumed_exactly_once() {
    let pending = Arc::new(DashMap::new());
    let (response_tx, response_rx) = oneshot::channel();
    pending.insert(
        user_question_pending_key("session-a", "call-1"),
        PendingUserQuestion {
            questions: vec![question(false)],
            response_tx,
        },
    );

    let mut answers = BTreeMap::new();
    answers.insert(
        "Features".to_string(),
        AskUserQuestionAnswer::Single("custom user input".to_string()),
    );
    respond_pending_user_question(&pending, "session-a", "call-1", answers).unwrap();
    assert_eq!(
        response_rx.await.unwrap(),
        AskUserQuestionResponse::Answered {
            answers: BTreeMap::from([(
                "Features".to_string(),
                AskUserQuestionAnswer::Single("custom user input".to_string()),
            )]),
        }
    );

    let error = respond_pending_user_question(
        &pending,
        "session-a",
        "call-1",
        BTreeMap::from([(
            "Features".to_string(),
            AskUserQuestionAnswer::Single("stale".to_string()),
        )]),
    )
    .unwrap_err();
    assert!(error.contains("No pending user question"));
}

#[tokio::test]
async fn cancellation_resolves_only_the_requested_session() {
    let pending = Arc::new(DashMap::new());
    let (a_tx, a_rx) = oneshot::channel();
    let (b_tx, mut b_rx) = oneshot::channel();
    pending.insert(
        user_question_pending_key("session-a", "call-1"),
        PendingUserQuestion {
            questions: vec![question(true)],
            response_tx: a_tx,
        },
    );
    pending.insert(
        user_question_pending_key("session-b", "call-1"),
        PendingUserQuestion {
            questions: vec![question(true)],
            response_tx: b_tx,
        },
    );

    assert_eq!(
        cancel_pending_user_questions(&pending, Some("session-a")),
        1
    );
    assert_eq!(a_rx.await.unwrap(), AskUserQuestionResponse::Cancelled);
    assert!(b_rx.try_recv().is_err());
    assert_eq!(cancel_pending_user_questions(&pending, None), 1);
    assert_eq!(b_rx.await.unwrap(), AskUserQuestionResponse::Cancelled);
}

#[test]
fn frontend_contract_always_exposes_custom_input() {
    let html = include_str!("../frontend/index.html");
    let css = include_str!("../frontend/styles/components/overlays.css");
    let source = include_str!("../frontend/app.js");

    assert!(html.contains("id=\"questionPanel\""));
    assert!(html.contains("id=\"questionPanelTitle\""));
    assert!(css.contains("padding: 6px 8px;"));
    assert!(css.contains("min-height: 28px;"));
    assert!(css.contains(".question-panel-error:empty { display: none; }"));
    assert!(css.contains("align-items: center;"));
    assert!(css.contains("margin-left: auto;"));
    assert!(html.contains("id=\"questionPanelOtherInput\""));
    assert!(!html.contains("id=\"questionPanelProgress\""));
    assert!(!html.contains("id=\"questionPanelQuestion\""));
    assert!(source.contains("question-request"));
    assert!(source.contains("respond_user_question"));
    assert!(source.contains("title.textContent = '['"));
    assert!(source.contains("'Other'"));
    assert!(source.contains("data-option-number"));
    assert!(source.contains("clearQuestionOtherInput"));
    assert!(source.contains("Done (D)"));
    assert!(!source.contains("allowOther"));
}
