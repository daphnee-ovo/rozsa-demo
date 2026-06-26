use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use rozsa_model::oauth::openai_codex::{
    build_auth_url, extract_account_id_from_jwt, generate_random_state, parse_authorization_input,
};
use rozsa_model::oauth::types::OAuthLoginError;

#[test]
fn test_build_auth_url() {
    let challenge = "test_challenge";
    let state = "test_state";
    let url = build_auth_url(challenge, state);
    assert!(url.contains("code_challenge=test_challenge"));
    assert!(url.contains("state=test_state"));
    assert!(url.contains("client_id="));
    assert!(url.contains("response_type=code"));
    assert!(url.contains("id_token_add_organizations=true"));
    assert!(url.contains("codex_cli_simplified_flow=true"));
    assert!(url.contains("originator=pi"));
}

#[test]
fn test_parse_authorization_input_raw_code() {
    let result = parse_authorization_input("abc123", "expected_state");
    assert!(result.is_ok());
    let (code, state) = result.unwrap();
    assert_eq!(code, "abc123");
    assert_eq!(state, "expected_state");
}

#[test]
fn test_parse_authorization_input_query_string() {
    let result = parse_authorization_input("code=abc123&state=test_state", "test_state");
    assert!(result.is_ok());
    let (code, state) = result.unwrap();
    assert_eq!(code, "abc123");
    assert_eq!(state, "test_state");
}

#[test]
fn test_parse_authorization_input_hash_format() {
    let result = parse_authorization_input("abc123#test_state", "test_state");
    assert!(result.is_ok());
    let (code, state) = result.unwrap();
    assert_eq!(code, "abc123");
    assert_eq!(state, "test_state");
}

#[test]
fn test_parse_authorization_input_url() {
    let result = parse_authorization_input(
        "http://localhost:1455/auth/callback?code=abc123&state=test_state",
        "test_state",
    );
    assert!(result.is_ok());
    let (code, state) = result.unwrap();
    assert_eq!(code, "abc123");
    assert_eq!(state, "test_state");
}

#[test]
fn test_parse_authorization_input_state_mismatch() {
    let result = parse_authorization_input("code=abc123&state=wrong", "expected");
    assert!(result.is_err());
    match result.unwrap_err() {
        OAuthLoginError::CallbackServer(msg) => assert!(msg.contains("state mismatch")),
        _ => panic!("expected CallbackServer error"),
    }
}

#[test]
fn test_extract_account_id_from_jwt() {
    let payload = r#"{"https://api.openai.com/auth.chatgpt_account_id":"test-account-123"}"#;
    let payload_b64 = URL_SAFE_NO_PAD.encode(payload.as_bytes());
    let token = format!("header.{}.signature", payload_b64);

    let account_id = extract_account_id_from_jwt(&token);
    assert_eq!(account_id, Some("test-account-123".to_string()));
}

#[test]
fn test_extract_account_id_missing_field() {
    let payload = r#"{"sub":"user123"}"#;
    let payload_b64 = URL_SAFE_NO_PAD.encode(payload.as_bytes());
    let token = format!("header.{}.signature", payload_b64);

    let account_id = extract_account_id_from_jwt(&token);
    assert_eq!(account_id, None);
}

#[test]
fn test_extract_account_id_invalid_jwt() {
    let account_id = extract_account_id_from_jwt("not.a.valid.jwt");
    assert_eq!(account_id, None);
}

#[test]
fn test_generate_random_state() {
    let state1 = generate_random_state();
    let state2 = generate_random_state();
    assert_eq!(state1.len(), 32);
    assert_ne!(state1, state2);
}
