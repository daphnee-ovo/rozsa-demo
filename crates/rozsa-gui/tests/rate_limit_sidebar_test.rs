#[test]
fn native_sidebar_renders_codex_oauth_rate_limits_from_sidebar_state() {
    let html = include_str!("../frontend/sidebar.html");
    let js = include_str!("../frontend/sidebar.js");
    let state = include_str!("../src/state.rs");
    let commands = include_str!("../src/commands.rs");

    for id in [
        "sidebarQuotaGroup",
        "sidebarQuotaHour",
        "sidebarQuotaHourBar",
        "sidebarQuotaWeekRow",
        "sidebarQuotaWeek",
        "sidebarQuotaWeekBar",
    ] {
        assert!(html.contains(&format!("id=\"{id}\"")), "missing {id}");
    }
    assert!(js.contains("function renderSidebarQuota(snapshot)"));
    assert!(js.contains("snapshot.showQuota"));
    assert!(js.contains("snapshot.showWeeklyQuota"));
    assert!(js.contains("snapshot.quota?.primary"));
    assert!(js.contains("snapshot.quota?.secondary"));
    assert!(html.contains(".quota-row[hidden], .quota-group[hidden] { display: none; }"));
    assert!(state.contains("pub show_quota: bool"));
    assert!(state.contains("pub show_weekly_quota: bool"));
    assert!(state.contains("provider.as_str() == \"codex-oauth\""));
    assert!(commands.contains("appearance_show_rate_limits"));
    assert!(commands.contains("appearance_show_weekly_rate_limit"));
}
