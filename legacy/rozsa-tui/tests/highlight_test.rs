use rozsa_tui::util::highlight::highlight_code;

#[test]
fn highlight_rust_code() {
    let code = "fn main() {\n    println!(\"hello\");\n}";
    let result = highlight_code(code, "rs");
    assert!(result.is_some());
    let lines = result.unwrap();
    assert_eq!(lines.len(), 3);
}

#[test]
fn unknown_language_returns_none() {
    let code = "some text";
    let result = highlight_code(code, "nonexistent_language_xyz");
    assert!(result.is_none());
}

#[test]
fn oversized_input_returns_none() {
    let code = "x\n".repeat(10_001);
    let result = highlight_code(&code, "rs");
    assert!(result.is_none());
}
