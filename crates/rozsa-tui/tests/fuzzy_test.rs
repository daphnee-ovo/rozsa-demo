use rozsa_tui::util::fuzzy::{fuzzy_filter, fuzzy_match};

#[test]
fn exact_match() {
    let m = fuzzy_match("hello", "hello");
    assert!(m.matches);
    assert!(m.score < -50.0); // exact match gets -100 bonus
}

#[test]
fn prefix_match() {
    let m = fuzzy_match("he", "hello");
    assert!(m.matches);
}

#[test]
fn no_match() {
    let m = fuzzy_match("xyz", "hello");
    assert!(!m.matches);
}

#[test]
fn case_insensitive() {
    let m = fuzzy_match("HE", "hello");
    assert!(m.matches);
}

#[test]
fn word_boundary_bonus() {
    let m1 = fuzzy_match("fc", "fooConfig");
    let m2 = fuzzy_match("fc", "function_call");
    // word boundary: "function_call" has f at start + c at _boundary
    assert!(m1.matches);
    assert!(m2.matches);
    // lower score = better
    assert!(m2.score <= m1.score, "word boundary should score better (lower): m2={} vs m1={}", m2.score, m1.score);
}

#[test]
fn consecutive_bonus() {
    // 比较连续匹配 vs 分散匹配（无 word boundary 干扰）
    let m1 = fuzzy_match("abc", "abcdef");    // 连续，开头
    let m2 = fuzzy_match("abc", "axxbxxcxx"); // 分散，有 gap penalty
    assert!(m1.matches);
    assert!(m2.matches);
    assert!(m1.score < m2.score, "consecutive should score better (lower): m1={} vs m2={}", m1.score, m2.score);
}

#[test]
fn filter_sorts_by_score() {
    let items = vec!["config.yaml", "my_config", "configure", "conifg"];
    let results = fuzzy_filter(&items, "config", |s| s);
    assert!(!results.is_empty());
    // "config.yaml" 或 "configure" 应排在前面（有完整连续匹配）
    let first_item = items[results[0].0];
    assert!(first_item.contains("config"));
}

#[test]
fn positions_tracking() {
    let m = fuzzy_match("hlo", "hello");
    assert!(m.matches);
    assert_eq!(m.positions, vec![0, 2, 4]);
}
