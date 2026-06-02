// components/session_search.rs — 会话搜索
//
// Internal Framework:
// session_search.rs
// └── filter_sessions()  按条件过滤会话列表
//
// Related Docs:
// - [TUI Design](../../../docs/rozsa_framework.md#rozsa-tui--ratatui-终端前端)

// session_search.rs
// ├── ParsedQuery          # 解析后的搜索查询
// ├── parse_query()        # 解析搜索字符串为结构化查询
// ├── match_session()      # 匹配单条 session，返回 (matches, score)
// └── filter_sessions()    # 过滤+评分入口
//
// Related: [session-selector-search.ts](../../coding-agent/src/modes/interactive/components/session-selector-search.ts)

use regex::Regex;

use crate::fuzzy::fuzzy_match;
use super::session_selector::SessionEntry;

#[derive(Debug)]
enum ParsedQuery {
    /// 普通 token 模糊匹配（空格分词）
    Tokens(Vec<String>),
    /// 正则匹配 re:<pattern>
    Regex(Regex),
    /// 精确短语匹配 "phrase"
    Phrase(String),
}

fn parse_query(input: &str) -> ParsedQuery {
    let trimmed = input.trim();

    // re:<pattern> — 正则模式
    if let Some(pattern) = trimmed.strip_prefix("re:") {
        let pat = pattern.trim();
        if let Ok(re) = Regex::new(&format!("(?i){pat}")) {
            return ParsedQuery::Regex(re);
        }
        // 正则无效时回退为 token 匹配
    }

    // "phrase" — 精确短语模式
    if trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() > 2 {
        let phrase = &trimmed[1..trimmed.len() - 1];
        return ParsedQuery::Phrase(phrase.to_lowercase());
    }

    // 默认：token 模糊匹配
    let tokens = trimmed
        .split_whitespace()
        .map(|s| s.to_lowercase())
        .collect();
    ParsedQuery::Tokens(tokens)
}

/// 构建搜索 haystack：name + firstMessage + cwd + allMessagesText
fn build_haystack(entry: &SessionEntry) -> String {
    let mut haystack = String::new();
    if let Some(ref name) = entry.name {
        haystack.push_str(name);
        haystack.push(' ');
    }
    haystack.push_str(&entry.first_message);
    haystack.push(' ');
    haystack.push_str(&entry.cwd);
    haystack.push(' ');
    haystack.push_str(&entry.all_messages_text);
    haystack
}

/// 匹配单条 session，返回 (matches, score)
fn match_session(entry: &SessionEntry, query: &ParsedQuery) -> (bool, f64) {
    match query {
        ParsedQuery::Tokens(tokens) => {
            if tokens.is_empty() {
                return (true, 0.0);
            }
            let haystack = build_haystack(entry);
            let mut total_score = 0.0;
            for token in tokens {
                let m = fuzzy_match(token, &haystack);
                if !m.matches {
                    return (false, 0.0);
                }
                total_score += m.score;
            }
            // name 匹配额外加分（subsequence fuzzy）
            if let Some(ref name) = entry.name {
                for token in tokens {
                    let nm = fuzzy_match(token, name);
                    if nm.matches {
                        total_score -= 20.0; // lower score = better ranking
                    }
                }
            }
            (true, total_score)
        }
        ParsedQuery::Regex(re) => {
            let haystack = build_haystack(entry);
            let matches = re.is_match(&haystack);
            (matches, if matches { 1.0 } else { 0.0 })
        }
        ParsedQuery::Phrase(phrase) => {
            let haystack = build_haystack(entry).to_lowercase();
            // normalize whitespace
            let normalized: String = haystack
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            let matches = normalized.contains(phrase.as_str());
            (matches, if matches { 1.0 } else { 0.0 })
        }
    }
}

/// 过滤 entries 中指定 indices 的会话，返回 (entry_index, score)
pub fn filter_sessions(
    entries: &[SessionEntry],
    indices: &[usize],
    query: &str,
) -> Vec<(usize, f64)> {
    let parsed = parse_query(query);
    indices
        .iter()
        .filter_map(|&i| {
            let (matches, score) = match_session(&entries[i], &parsed);
            if matches { Some((i, score)) } else { None }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
