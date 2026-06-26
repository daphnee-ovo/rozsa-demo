use rozsa_tui::data::session_search::{filter_sessions, match_session, parse_query};
use rozsa_tui::panels::session_selector::SessionEntry;

fn make_entry(name: Option<&str>, first_msg: &str, all_text: &str) -> SessionEntry {
    SessionEntry {
        path: "/tmp/test.jsonl".to_string(),
        name: name.map(|s| s.to_string()),
        first_message: first_msg.to_string(),
        cwd: "/home/user/project".to_string(),
        message_count: 5,
        last_modified: "2026-05-29T10:00:00Z".to_string(),
        parent_session_path: None,
        all_messages_text: all_text.to_string(),
    }
}

#[test]
fn test_token_match() {
    let entry = make_entry(Some("auth refactor"), "fix login bug", "auth login token refresh");
    let parsed = parse_query("auth login");
    let (matches, _score) = match_session(&entry, &parsed);
    assert!(matches);
}

#[test]
fn test_token_no_match() {
    let entry = make_entry(Some("auth refactor"), "fix login bug", "auth login");
    let parsed = parse_query("database migration");
    let (matches, _) = match_session(&entry, &parsed);
    assert!(!matches);
}

#[test]
fn test_regex_match() {
    let entry = make_entry(None, "implement OAuth2 flow", "oauth2 token exchange");
    let parsed = parse_query("re:oauth\\d");
    let (matches, _) = match_session(&entry, &parsed);
    assert!(matches);
}

#[test]
fn test_regex_no_match() {
    let entry = make_entry(None, "fix button style", "css styling");
    let parsed = parse_query("re:oauth\\d");
    let (matches, _) = match_session(&entry, &parsed);
    assert!(!matches);
}

#[test]
fn test_phrase_match() {
    let entry = make_entry(None, "fix the login flow", "user login flow broken");
    let parsed = parse_query("\"login flow\"");
    let (matches, _) = match_session(&entry, &parsed);
    assert!(matches);
}

#[test]
fn test_phrase_no_match() {
    let entry = make_entry(None, "login was fixed, flow works", "login xyz flow");
    // "login flow" 作为精确短语不应匹配 "login xyz flow"（normalize 后仍不连续）
    let parsed = parse_query("\"login flow\"");
    let (matches, _) = match_session(&entry, &parsed);
    // 注意：normalize whitespace 后 "login xyz flow" 包含 "login" 和 "flow" 但不包含 "login flow"
    assert!(!matches);
}

// --- Fuzzy subsequence matching ---

#[test]
fn test_fuzzy_subsequence_match() {
    // "authrf" 应该 fuzzy 匹配 "auth refactor"（子序列）
    let entry = make_entry(Some("auth refactor"), "fix login", "");
    let parsed = parse_query("authrf");
    let (matches, _) = match_session(&entry, &parsed);
    assert!(matches);
}

#[test]
fn test_fuzzy_no_match_unrelated() {
    let entry = make_entry(Some("database"), "migration", "sql");
    let parsed = parse_query("xyz123");
    let (matches, _) = match_session(&entry, &parsed);
    assert!(!matches);
}

#[test]
fn test_name_bonus_scoring() {
    // 匹配 name 字段的 token 应该获得更好的分数（更低）
    let entry_with_name = make_entry(Some("deploy"), "fix deploy script", "deploy production");
    let entry_no_name = make_entry(None, "deploy production fix", "deploy script");
    let parsed = parse_query("deploy");
    let (m1, score1) = match_session(&entry_with_name, &parsed);
    let (m2, score2) = match_session(&entry_no_name, &parsed);
    assert!(m1);
    assert!(m2);
    // entry_with_name 有 name 加分 (-20)，score 应该更低
    assert!(score1 < score2);
}

#[test]
fn test_multiple_tokens_all_must_match() {
    let entry = make_entry(None, "auth login handler", "token refresh");
    // 两个 token 都必须 fuzzy 匹配
    let parsed = parse_query("auth token");
    let (matches, _) = match_session(&entry, &parsed);
    assert!(matches);

    let parsed_fail = parse_query("auth zzzzz");
    let (matches_fail, _) = match_session(&entry, &parsed_fail);
    assert!(!matches_fail);
}

#[test]
fn test_filter_sessions_integration() {
    let entries = vec![
        make_entry(Some("fix auth"), "login bug", "authentication module"),
        make_entry(Some("add tests"), "unit tests", "vitest coverage"),
        make_entry(None, "deploy script", "k8s rollout"),
    ];
    let indices: Vec<usize> = (0..entries.len()).collect();
    // "authentication" 只可能 fuzzy 匹配第一条
    let results = filter_sessions(&entries, &indices, "authentication");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].0, 0);
}

#[test]
fn test_empty_query_matches_all() {
    let entry = make_entry(Some("anything"), "text", "more text");
    let parsed = parse_query("");
    let (matches, score) = match_session(&entry, &parsed);
    assert!(matches);
    assert_eq!(score, 0.0);
}
