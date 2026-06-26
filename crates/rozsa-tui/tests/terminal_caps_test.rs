use rozsa_tui::util::terminal_caps::detect;

#[test]
fn default_detection_no_crash() {
    let caps = detect();
    // 在测试环境下不会 panic
    let _ = caps.true_color;
    let _ = caps.hyperlinks;
    let _ = caps.images;
}
