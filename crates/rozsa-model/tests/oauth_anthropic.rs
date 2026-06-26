use rozsa_model::oauth::anthropic::{build_auth_url, parse_authorization_input};
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
        "http://127.0.0.1:53692/callback?code=abc123&state=test_state",
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
