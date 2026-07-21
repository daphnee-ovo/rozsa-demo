use rozsa_app::session::manager::SessionManager;
use rozsa_core::messages::AgentMessage;
use rozsa_gui::state::{SessionTab, session_display_name};
use rozsa_model::types::{Message, UserContent, UserMessage};

fn user_message(text: &str) -> AgentMessage {
    AgentMessage::standard(Message::User(UserMessage {
        content: UserContent::Text(text.to_string()),
        display_text: None,
        timestamp: 0,
    }))
}

#[test]
fn selected_session_uses_name_then_preview_then_untitled() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("session.jsonl");
    let mut manager = SessionManager::create(
        &path,
        "session".to_string(),
        temp.path().to_string_lossy().to_string(),
        None,
    )
    .unwrap();
    manager
        .append_session_info(Some("Manual name".to_string()))
        .unwrap();
    let named = SessionTab::Loaded {
        path: path.to_string_lossy().to_string(),
        messages: vec![user_message("First message")],
    };
    assert_eq!(session_display_name(&named), "Manual name");

    let preview = SessionTab::Loaded {
        path: temp
            .path()
            .join("not-materialized.jsonl")
            .to_string_lossy()
            .to_string(),
        messages: vec![user_message(&"x".repeat(55))],
    };
    assert_eq!(
        session_display_name(&preview),
        format!("{}...", "x".repeat(50))
    );

    let empty = SessionTab::Loaded {
        path: temp
            .path()
            .join("empty.jsonl")
            .to_string_lossy()
            .to_string(),
        messages: Vec::new(),
    };
    assert_eq!(session_display_name(&empty), "Untitled");
}

#[test]
fn frontend_places_thinking_level_next_to_model_and_reserves_brand_for_no_session() {
    let html = include_str!("../frontend/index.html");
    let app = include_str!("../frontend/app.js");
    let model = html.find("id=\"modelSelector\"").unwrap();
    let thinking = html.find("id=\"thinkingLevel\"").unwrap();

    assert!(thinking > model);
    assert!(app.contains("snap.sessionName || 'Rózsa'"));
    assert!(!html.contains("data-od-id=\"perm-badge\""));
}

#[test]
fn title_generation_starts_before_the_main_prompt_and_exposes_small_models() {
    let commands = include_str!("../src/commands.rs");
    let html = include_str!("../frontend/index.html");
    let app = include_str!("../frontend/app.js");
    let send = commands.find("pub async fn send_message").unwrap();
    let naming = commands[send..]
        .find("spawn_session_name_generation(")
        .unwrap();
    let prompt = commands[send..]
        .find(".prompt_with_prefix_blocks(")
        .unwrap();

    assert!(naming < prompt);
    assert!(commands.contains("is_initial_session_name_candidate().await"));
    assert!(commands.contains("provider_available()"));
    assert!(html.contains("id=\"settingsSmallModelSelect\""));
    assert!(!app.contains("models.filter(model => !model.reasoning)"));
    assert!(app.contains("saveSetting('small_model', smallSelect.value)"));
}
