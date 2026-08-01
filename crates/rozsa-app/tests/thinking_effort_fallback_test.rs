use std::collections::HashMap;
use std::fs;
use std::sync::{Arc, Mutex};

use rozsa_app::agent_session::{
    ModelStream, model_stream_with_thinking_effort_fallback, normalize_thinking_effort,
    supported_thinking_efforts, thinking_effort_attempt_values,
};
use rozsa_app::model_registry::ModelRegistry;
use rozsa_model::event_stream::{EventStream, create_event_stream};
use rozsa_model::providers::common::is_explicit_unsupported_thinking_effort_error;
use rozsa_model::types::{
    CacheRetention, Context, Model, SimpleStreamOptions, StopReason, StreamEvent, StreamOptions,
    ThinkingEffort, Transport,
};
use tempfile::tempdir;

fn write_models_config(dir: &std::path::Path) -> std::path::PathBuf {
    let path = dir.join("demo.json");
    fs::write(
        &path,
        r#"{
            "providers": {
                "demo": {
                    "baseUrl": "https://api.example.test/v1",
                    "apiKey": "DEMO_API_KEY",
                    "api": "openai-responses",
                    "headers": { "X-Unrelated": "preserved" },
                    "models": [{
                        "id": "demo-reasoner",
                        "reasoning": true,
                        "input": ["text"],
                        "contextWindow": 32000,
                        "maxTokens": 4096
                    }]
                }
            }
        }"#,
    )
    .unwrap();
    path
}

fn options(effort: ThinkingEffort) -> SimpleStreamOptions {
    SimpleStreamOptions {
        base: StreamOptions {
            temperature: None,
            max_tokens: None,
            api_key: None,
            transport: Transport::Auto,
            cache_retention: CacheRetention::None,
            session_id: None,
            headers: None,
            timeout_ms: None,
            max_retries: None,
            max_retry_delay_ms: None,
            metadata: None,
        },
        reasoning: Some(effort),
        thinking_effort_budgets: None,
        tool_choice: None,
    }
}

fn error_stream(model: &Model, message: &str) -> EventStream<StreamEvent> {
    let (sender, stream) = create_event_stream();
    let mut error = rozsa_model::providers::common::create_output(model, model.api.clone());
    error.stop_reason = StopReason::Error;
    error.error_message = Some(message.to_string());
    sender.push(StreamEvent::Error {
        reason: StopReason::Error,
        error,
    });
    drop(sender);
    stream
}

fn success_stream(model: &Model) -> EventStream<StreamEvent> {
    let (sender, stream) = create_event_stream();
    sender.push(StreamEvent::Done {
        reason: StopReason::Stop,
        message: rozsa_model::providers::common::create_output(model, model.api.clone()),
    });
    drop(sender);
    stream
}

fn attempt_value(model: &Model, effort: ThinkingEffort) -> String {
    model
        .thinking_effort_map
        .as_ref()
        .and_then(|map| map.get(&effort))
        .and_then(Clone::clone)
        .unwrap()
}

#[test]
fn supported_efforts_follow_model_config_and_fall_back_downward() {
    let temp = tempdir().unwrap();
    let _path = write_models_config(temp.path());
    let model = ModelRegistry::load_from_dir(temp.path())
        .unwrap()
        .resolve("demo", "demo-reasoner")
        .unwrap();

    let mut max_to_xhigh = model.clone();
    max_to_xhigh.thinking_effort_map = Some(HashMap::from([
        (ThinkingEffort::Max, None),
        (ThinkingEffort::XHigh, Some("xhigh".to_string())),
    ]));
    assert_eq!(
        supported_thinking_efforts(&max_to_xhigh),
        vec![
            ThinkingEffort::Off,
            ThinkingEffort::Low,
            ThinkingEffort::Medium,
            ThinkingEffort::High,
            ThinkingEffort::XHigh,
        ]
    );
    assert_eq!(
        normalize_thinking_effort(&max_to_xhigh, ThinkingEffort::Max),
        ThinkingEffort::XHigh
    );

    let mut max_to_high = model;
    max_to_high.thinking_effort_map = Some(HashMap::from([
        (ThinkingEffort::Max, None),
        (ThinkingEffort::XHigh, None),
        (ThinkingEffort::High, Some("high".to_string())),
    ]));
    assert_eq!(
        normalize_thinking_effort(&max_to_high, ThinkingEffort::Max),
        ThinkingEffort::High
    );
    assert_eq!(
        normalize_thinking_effort(&max_to_high, ThinkingEffort::XHigh),
        ThinkingEffort::High
    );

    max_to_high.reasoning = false;
    assert_eq!(
        supported_thinking_efforts(&max_to_high),
        vec![ThinkingEffort::Off]
    );
    assert_eq!(
        normalize_thinking_effort(&max_to_high, ThinkingEffort::Max),
        ThinkingEffort::Off
    );
}

#[tokio::test]
async fn successful_light_fallback_is_persisted_and_preferred() {
    let temp = tempdir().unwrap();
    let path = write_models_config(temp.path());
    let model = ModelRegistry::load_from_dir(temp.path())
        .unwrap()
        .resolve("demo", "demo-reasoner")
        .unwrap();
    let calls = Arc::new(Mutex::new(Vec::new()));
    let attempt_stream: ModelStream = {
        let calls = calls.clone();
        Arc::new(move |model, _, _| {
            let value = attempt_value(model, ThinkingEffort::Low);
            calls.lock().unwrap().push(value.clone());
            if value == "light" {
                success_stream(model)
            } else {
                error_stream(
                    model,
                    "Provider HTTP error (400): reasoning_effort is not supported",
                )
            }
        })
    };
    let fallback =
        model_stream_with_thinking_effort_fallback(temp.path().to_path_buf(), attempt_stream);
    let mut stream = fallback(
        &model,
        &Context {
            system_prompt: None,
            messages: vec![],
            tools: vec![],
        },
        &options(ThinkingEffort::Low),
    );
    assert!(matches!(
        stream.next().await,
        Some(StreamEvent::Done { .. })
    ));
    assert_eq!(*calls.lock().unwrap(), vec!["low", "light"]);

    let persisted: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(
        persisted["providers"]["demo"]["headers"]["X-Unrelated"],
        "preserved"
    );
    assert_eq!(
        persisted["providers"]["demo"]["models"][0]["thinkingEffortMap"]["low"],
        "light"
    );

    let reloaded = ModelRegistry::load_from_dir(temp.path()).unwrap();
    let model = reloaded.resolve("demo", "demo-reasoner").unwrap();
    assert_eq!(
        thinking_effort_attempt_values(&model, ThinkingEffort::Low),
        vec!["light", "low", "minimal"]
    );
}

#[tokio::test]
async fn all_low_candidates_rejected_disables_low_without_changing_other_efforts() {
    let temp = tempdir().unwrap();
    let path = write_models_config(temp.path());
    let model = ModelRegistry::load_from_dir(temp.path())
        .unwrap()
        .resolve("demo", "demo-reasoner")
        .unwrap();
    let calls = Arc::new(Mutex::new(Vec::new()));
    let attempt_stream: ModelStream = {
        let calls = calls.clone();
        Arc::new(move |model, _, _| {
            calls
                .lock()
                .unwrap()
                .push(attempt_value(model, ThinkingEffort::Low));
            error_stream(
                model,
                "Provider HTTP error (422): reasoning_effort is unsupported",
            )
        })
    };
    let fallback =
        model_stream_with_thinking_effort_fallback(temp.path().to_path_buf(), attempt_stream);
    let mut stream = fallback(
        &model,
        &Context {
            system_prompt: None,
            messages: vec![],
            tools: vec![],
        },
        &options(ThinkingEffort::Low),
    );
    assert!(matches!(
        stream.next().await,
        Some(StreamEvent::Error { .. })
    ));
    assert_eq!(*calls.lock().unwrap(), vec!["low", "light", "minimal"]);

    let reloaded = ModelRegistry::load_from_dir(temp.path()).unwrap();
    let model = reloaded.resolve("demo", "demo-reasoner").unwrap();
    assert!(thinking_effort_attempt_values(&model, ThinkingEffort::Low).is_empty());
    assert_eq!(
        thinking_effort_attempt_values(&model, ThinkingEffort::Medium),
        vec!["medium"]
    );
    let persisted: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
    assert!(
        persisted["providers"]["demo"]["models"][0]["thinkingEffortMap"]
            .get("medium")
            .is_none()
    );
}

#[tokio::test]
async fn rejected_high_is_disabled_without_affecting_medium() {
    let temp = tempdir().unwrap();
    let path = write_models_config(temp.path());
    let model = ModelRegistry::load_from_dir(temp.path())
        .unwrap()
        .resolve("demo", "demo-reasoner")
        .unwrap();
    let mut mapped_high = model.clone();
    mapped_high.thinking_effort_map = Some(HashMap::from([(
        ThinkingEffort::High,
        Some("strong".to_string()),
    )]));
    assert_eq!(
        thinking_effort_attempt_values(&mapped_high, ThinkingEffort::High),
        vec!["strong"]
    );
    let calls = Arc::new(Mutex::new(Vec::new()));
    let attempt_stream: ModelStream = {
        let calls = calls.clone();
        Arc::new(move |model, _, _| {
            calls
                .lock()
                .unwrap()
                .push(attempt_value(model, ThinkingEffort::High));
            error_stream(
                model,
                "Provider HTTP error (400): thinking effort is not supported",
            )
        })
    };
    let fallback =
        model_stream_with_thinking_effort_fallback(temp.path().to_path_buf(), attempt_stream);
    let mut stream = fallback(
        &model,
        &Context {
            system_prompt: None,
            messages: vec![],
            tools: vec![],
        },
        &options(ThinkingEffort::High),
    );
    assert!(matches!(
        stream.next().await,
        Some(StreamEvent::Error { .. })
    ));
    assert_eq!(*calls.lock().unwrap(), vec!["high"]);

    let reloaded = ModelRegistry::load_from_dir(temp.path()).unwrap();
    let model = reloaded.resolve("demo", "demo-reasoner").unwrap();
    assert!(thinking_effort_attempt_values(&model, ThinkingEffort::High).is_empty());
    assert_eq!(
        thinking_effort_attempt_values(&model, ThinkingEffort::Medium),
        vec!["medium"]
    );
    let persisted: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
    assert!(persisted["providers"]["demo"]["models"][0]["thinkingEffortMap"]["high"].is_null());
}

#[tokio::test]
async fn unrelated_failures_are_not_retried_or_persisted() {
    assert!(!is_explicit_unsupported_thinking_effort_error(
        "Provider HTTP error (401): reasoning_effort is unsupported"
    ));
    assert!(!is_explicit_unsupported_thinking_effort_error(
        "Provider HTTP error (400): model does not exist"
    ));
    assert!(is_explicit_unsupported_thinking_effort_error(
        "Provider HTTP error (422): reasoning_effort is not supported by this model"
    ));

    let temp = tempdir().unwrap();
    let path = write_models_config(temp.path());
    let model = ModelRegistry::load_from_dir(temp.path())
        .unwrap()
        .resolve("demo", "demo-reasoner")
        .unwrap();
    let calls = Arc::new(Mutex::new(Vec::new()));
    let attempt_stream: ModelStream = {
        let calls = calls.clone();
        Arc::new(move |model, _, _| {
            calls
                .lock()
                .unwrap()
                .push(attempt_value(model, ThinkingEffort::Low));
            error_stream(
                model,
                "Provider HTTP error (401): reasoning_effort is unsupported",
            )
        })
    };
    let fallback =
        model_stream_with_thinking_effort_fallback(temp.path().to_path_buf(), attempt_stream);
    let mut stream = fallback(
        &model,
        &Context {
            system_prompt: None,
            messages: vec![],
            tools: vec![],
        },
        &options(ThinkingEffort::Low),
    );
    assert!(matches!(
        stream.next().await,
        Some(StreamEvent::Error { .. })
    ));
    assert_eq!(*calls.lock().unwrap(), vec!["low"]);
    let persisted: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
    assert!(
        persisted["providers"]["demo"]["models"][0]
            .get("thinkingEffortMap")
            .is_none()
    );
}
