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
fn composer_thinking_level_opens_a_model_aware_persisted_slider() {
    let html = include_str!("../frontend/index.html");
    let app = include_str!("../frontend/app.js");

    assert!(html.contains("id=\"thinkingLevel\""));
    assert!(html.contains("aria-haspopup=\"dialog\""));
    assert!(html.contains("id=\"thinkingLevelSlider\""));
    assert!(html.contains("type=\"range\""));
    assert!(html.contains("aria-label=\"Thinking level\""));
    assert!(app.contains("const THINKING_LEVEL_OPTIONS = Object.freeze("));
    assert!(app.contains("'off', 'minimal', 'low', 'medium', 'high', 'xhigh'"));
    assert!(app.contains("model && model.reasoning ? THINKING_LEVEL_OPTIONS"));
    assert!(app.contains("await saveSetting('thinking', option.value)"));
    assert!(app.contains("await saveSetting('thinking', 'off')"));
}

#[test]
fn thinking_slider_popover_is_not_clipped_by_the_composer_frame() {
    let html = include_str!("../frontend/index.html");
    let app = include_str!("../frontend/app.js");
    let settings_start = html
        .find("<!-- ============ 设置面板")
        .expect("settings boundary must exist");
    let main_end = html[..settings_start]
        .rfind("</main>")
        .expect("main content boundary must exist");
    let popover = html
        .find("id=\"thinkingLevelPopover\"")
        .expect("thinking popover must exist");

    assert!(
        popover > main_end,
        "popover must live outside the clipped composer"
    );
    assert!(html.contains(".thinking-level-popover {\n  position: fixed;"));
    assert!(app.contains("function positionThinkingLevelPopover()"));
    assert!(app.contains("popover.style.left ="));
    assert!(app.contains("popover.style.top ="));
}

#[test]
fn thinking_slider_uses_compact_composer_scale() {
    let html = include_str!("../frontend/index.html");

    assert!(html.contains("width: min(320px, calc(100vw - 32px));"));
    assert!(
        html.contains(".thinking-level-slider::-webkit-slider-runnable-track {\n  height: 28px;")
    );
    assert!(html.contains(
        ".thinking-level-slider::-webkit-slider-thumb {\n  width: 38px;\n  height: 38px;"
    ));
    assert!(!html.contains("width: min(420px, calc(100vw - 32px));"));
    assert!(!html.contains("height: 54px;"));
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
