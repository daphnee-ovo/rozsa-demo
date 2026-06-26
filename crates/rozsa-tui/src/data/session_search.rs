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

use crate::util::fuzzy::fuzzy_match;
use crate::panels::session_selector::SessionEntry;

#[derive(Debug)]
pub enum ParsedQuery {
    /// 普通 token 模糊匹配（空格分词）
    Tokens(Vec<String>),
    /// 正则匹配 re:<pattern>
    Regex(Regex),
    /// 精确短语匹配 "phrase"
    Phrase(String),
}

pub fn parse_query(input: &str) -> ParsedQuery {
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
pub fn match_session(entry: &SessionEntry, query: &ParsedQuery) -> (bool, f64) {
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

