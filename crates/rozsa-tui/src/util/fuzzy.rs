// fuzzy.rs
//
// Internal Framework:
// fuzzy.rs
// ├── FuzzyMatch          — 匹配结果（是否匹配 + 分数）
// ├── fuzzy_match()       — 单 query 匹配
// └── fuzzy_filter()      — 批量过滤+排序
//
// Related Docs:
// - [TS fuzzy.ts](../../../packages/tui/src/fuzzy.ts)
// - [Task T014](../../dev-doc/refactor/tui/task/task_2026-05-28_1.md)

/// 模糊匹配结果
#[derive(Debug, Clone)]
pub struct FuzzyMatch {
    pub matches: bool,
    pub score: f64,
    /// 匹配字符在 text 中的索引位置（用于高亮）
    pub positions: Vec<usize>,
}

/// 单次模糊匹配：query 中所有字符需按顺序出现在 text 中
pub fn fuzzy_match(query: &str, text: &str) -> FuzzyMatch {
    let query_lower: Vec<char> = query.to_lowercase().chars().collect();
    let text_lower: Vec<char> = text.to_lowercase().chars().collect();

    if query_lower.is_empty() {
        return FuzzyMatch { matches: true, score: 0.0, positions: vec![] };
    }
    if query_lower.len() > text_lower.len() {
        return FuzzyMatch { matches: false, score: 0.0, positions: vec![] };
    }

    let result = match_query(&query_lower, &text_lower);
    if result.matches {
        return result;
    }

    // alpha+numeric swap 尝试（如 "ls2" → "2ls"）
    if let Some(swapped) = try_swap_alpha_digits(&query_lower) {
        let swapped_result = match_query(&swapped, &text_lower);
        if swapped_result.matches {
            return FuzzyMatch {
                matches: true,
                score: swapped_result.score + 5.0,
                positions: swapped_result.positions,
            };
        }
    }

    result
}

fn match_query(query: &[char], text: &[char]) -> FuzzyMatch {
    let mut query_idx = 0;
    let mut score = 0.0_f64;
    let mut last_match_idx: Option<usize> = None;
    let mut consecutive = 0u32;
    let mut positions = Vec::new();

    for (i, &ch) in text.iter().enumerate() {
        if query_idx >= query.len() {
            break;
        }
        if ch == query[query_idx] {
            let is_word_boundary = i == 0 || matches!(
                text[i - 1], ' ' | '\t' | '-' | '_' | '.' | '/' | ':'
            );

            if let Some(last) = last_match_idx {
                if last == i - 1 {
                    consecutive += 1;
                    score -= consecutive as f64 * 5.0;
                } else {
                    consecutive = 0;
                    score += (i - last - 1) as f64 * 2.0;
                }
            }

            if is_word_boundary {
                score -= 10.0;
            }

            score += i as f64 * 0.1;
            last_match_idx = Some(i);
            positions.push(i);
            query_idx += 1;
        }
    }

    if query_idx < query.len() {
        return FuzzyMatch { matches: false, score: 0.0, positions: vec![] };
    }

    // exact match 加分
    if query == text {
        score -= 100.0;
    }

    FuzzyMatch { matches: true, score, positions }
}

/// 尝试交换 alpha 和 digit 部分（如 "abc123" → "123abc"）
fn try_swap_alpha_digits(query: &[char]) -> Option<Vec<char>> {
    let s: String = query.iter().collect();
    // 尝试 letters+digits
    if let Some(split) = s.find(|c: char| c.is_ascii_digit()) {
        let (letters, digits) = s.split_at(split);
        if !letters.is_empty() && digits.chars().all(|c| c.is_ascii_digit()) {
            let swapped = format!("{digits}{letters}");
            return Some(swapped.chars().collect());
        }
    }
    // 尝试 digits+letters
    if let Some(split) = s.find(|c: char| c.is_ascii_alphabetic()) {
        let (digits, letters) = s.split_at(split);
        if !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()) {
            let swapped = format!("{letters}{digits}");
            return Some(swapped.chars().collect());
        }
    }
    None
}

/// 批量过滤+排序（支持空格分隔的多 token 匹配）
pub fn fuzzy_filter<T, F>(items: &[T], query: &str, get_text: F) -> Vec<(usize, f64)>
where
    F: Fn(&T) -> &str,
{
    let query = query.trim();
    if query.is_empty() {
        return (0..items.len()).map(|i| (i, 0.0)).collect();
    }

    let tokens: Vec<&str> = query.split_whitespace().collect();
    if tokens.is_empty() {
        return (0..items.len()).map(|i| (i, 0.0)).collect();
    }

    let mut results: Vec<(usize, f64)> = Vec::new();
    for (idx, item) in items.iter().enumerate() {
        let text = get_text(item);
        let mut total_score = 0.0;
        let mut all_match = true;

        for token in &tokens {
            let m = fuzzy_match(token, text);
            if m.matches {
                total_score += m.score;
            } else {
                all_match = false;
                break;
            }
        }

        if all_match {
            results.push((idx, total_score));
        }
    }

    results.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    results
}
