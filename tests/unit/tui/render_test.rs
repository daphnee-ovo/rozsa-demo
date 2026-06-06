// 渲染逻辑集成测试 — 使用 ratatui TestBackend 验证 UI 输出
//
// 不需要真终端，不启动 socket，直接构造 AppState 然后断言渲染帧内容。

use rozsa_tui::{
    app::AppState, input::InputState, protocol::NativeUiState, theme::THEME, ui::render,
};
use ratatui::{backend::TestBackend, buffer::Buffer, Terminal};

fn make_terminal(width: u16, height: u16) -> Terminal<TestBackend> {
    let backend = TestBackend::new(width, height);
    Terminal::new(backend).unwrap()
}

fn default_ui_state() -> NativeUiState {
    serde_json::from_str(
        r#"{
        "appName": "rozsa",
        "version": "0.1.0",
        "cwd": "/home/user/project",
        "thinkingLevel": "medium",
        "isStreaming": false,
        "isCompacting": false,
        "messages": [],
        "pendingMessages": [],
        "status": {},
        "widgetsAbove": {},
        "widgetsBelow": {},
        "keybindings": {"tui.input.submit": ["enter"]}
    }"#,
    )
    .unwrap()
}

fn render_to_string(state: &AppState, input: &InputState, width: u16, height: u16) -> String {
    let buf = render_to_buffer(state, input, width, height);
    buffer_to_string(&buf)
}

fn render_to_buffer(state: &AppState, input: &InputState, width: u16, height: u16) -> Buffer {
    let mut terminal = make_terminal(width, height);
    terminal.draw(|frame| render(frame, state, input)).unwrap();
    terminal.backend().buffer().clone()
}

fn buffer_to_string(buf: &Buffer) -> String {
    let mut output = String::new();
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            output.push_str(buf.cell((x, y)).unwrap().symbol());
        }
        output.push('\n');
    }
    output
}

#[test]
fn test_sidebar_shows_app_name_and_model() {
    let mut state = AppState::new();
    state.ui = default_ui_state();
    state.ui.model = Some(rozsa_tui::protocol::ModelInfo {
        id: "claude-sonnet-4".to_string(),
        provider: "anthropic".to_string(),
    });
    let input = InputState::default();
    let output = render_to_string(&state, &input, 120, 30);
    // app name 和 model 信息在侧边栏显示
    assert!(output.contains("PI"), "sidebar should show app name");
    assert!(
        output.contains("claude-sonnet-4"),
        "sidebar should show model"
    );
}

#[test]
fn test_empty_state_no_crash() {
    let state = AppState::new();
    let input = InputState::default();
    let mut terminal = make_terminal(80, 24);
    terminal
        .draw(|frame| render(frame, &state, &input))
        .unwrap();
}

#[test]
fn test_messages_render_user_and_assistant() {
    let mut state = AppState::new();
    state.ui = default_ui_state();
    state.ui.messages = serde_json::from_str(
        r#"[
        {"role": "user", "content": [{"type": "text", "text": "Hello world"}]},
        {"role": "assistant", "content": [{"type": "text", "text": "Hi there!"}]}
    ]"#,
    )
    .unwrap();
    let input = InputState::default();
    let output = render_to_string(&state, &input, 120, 30);
    assert!(output.contains("Hello world"), "should show user message");
    assert!(
        output.contains("Hi there!"),
        "should show assistant message"
    );
}

#[test]
fn test_tool_call_shows_name_and_preview() {
    let mut state = AppState::new();
    state.ui = default_ui_state();
    state.ui.messages = serde_json::from_str(
        r#"[
        {"role": "assistant", "content": [
            {"type": "toolCall", "name": "bash", "arguments": {"command": "ls -la"}}
        ]}
    ]"#,
    )
    .unwrap();
    let input = InputState::default();
    let output = render_to_string(&state, &input, 120, 30);
    // Bash 工具用 Box 背景色 + $ command 格式渲染
    assert!(
        output.contains("$ ls -la"),
        "should show bash command with $ prefix"
    );
}

#[test]
fn test_thinking_hidden_when_disabled() {
    let mut state = AppState::new();
    state.ui = default_ui_state();
    state.thinking_visible = false;
    state.ui.messages = serde_json::from_str(
        r#"[
        {"role": "assistant", "content": [
            {"type": "thinking", "thinking": "secret thoughts here"},
            {"type": "text", "text": "visible answer"}
        ]}
    ]"#,
    )
    .unwrap();
    let input = InputState::default();
    let output = render_to_string(&state, &input, 120, 30);
    assert!(
        !output.contains("secret thoughts"),
        "thinking should be hidden"
    );
    assert!(
        output.contains("thinking hidden"),
        "should show hidden indicator"
    );
    assert!(output.contains("visible answer"), "text should still show");
}

#[test]
fn test_thinking_visible_when_enabled() {
    let mut state = AppState::new();
    state.ui = default_ui_state();
    state.thinking_visible = true;
    state.ui.messages = serde_json::from_str(
        r#"[
        {"role": "assistant", "content": [
            {"type": "thinking", "thinking": "reasoning about stuff"},
            {"type": "text", "text": "final answer"}
        ]}
    ]"#,
    )
    .unwrap();
    let input = InputState::default();
    let output = render_to_string(&state, &input, 120, 30);
    assert!(
        output.contains("reasoning about stuff"),
        "thinking should be visible"
    );
    assert!(output.contains("final answer"), "text should still show");
}

#[test]
fn test_notification_renders_with_level_color() {
    let mut state = AppState::new();
    state.ui = default_ui_state();
    state.notifications.push(rozsa_tui::app::Notification {
        level: "error".to_string(),
        message: "Something went wrong".to_string(),
        created_at: std::time::Instant::now(),
    });
    let input = InputState::default();
    let output = render_to_string(&state, &input, 120, 30);
    assert!(
        output.contains("Something went wrong"),
        "notification should render"
    );
}

#[test]
fn test_streaming_shows_working_indicator() {
    let mut state = AppState::new();
    state.ui = default_ui_state();
    state.ui.is_streaming = true;
    let input = InputState::default();
    let output = render_to_string(&state, &input, 120, 30);
    assert!(
        output.contains("Working..."),
        "should show streaming indicator above input"
    );
}

#[test]
fn test_compacting_shows_indicator() {
    let mut state = AppState::new();
    state.ui = default_ui_state();
    state.compacting = true;
    let input = InputState::default();
    let output = render_to_string(&state, &input, 120, 30);
    assert!(
        output.contains("Compacting session"),
        "should show compacting indicator"
    );
}

#[test]
fn test_retry_shows_countdown() {
    let mut state = AppState::new();
    state.ui = default_ui_state();
    state.retry = Some(rozsa_tui::app::RetryState {
        reason: "rate limited".to_string(),
        started_at: std::time::Instant::now(),
        total_seconds: 30,
    });
    let input = InputState::default();
    let output = render_to_string(&state, &input, 120, 30);
    assert!(
        output.contains("Retrying in"),
        "should show retry countdown"
    );
    assert!(output.contains("rate limited"), "should show retry reason");
}

#[test]
fn test_wide_terminal_shows_sidebar() {
    let mut state = AppState::new();
    state.ui = default_ui_state();
    state.ui.model = Some(rozsa_tui::protocol::ModelInfo {
        id: "claude-sonnet-4".to_string(),
        provider: "anthropic".to_string(),
    });
    let input = InputState::default();
    // 宽终端 (>=108) 应显示侧边栏
    let output = render_to_string(&state, &input, 130, 30);
    assert!(
        output.contains("CONTEXT"),
        "wide terminal should show sidebar with CONTEXT"
    );
}

#[test]
fn test_narrow_terminal_hides_sidebar() {
    let mut state = AppState::new();
    state.ui = default_ui_state();
    let input = InputState::default();
    // 窄终端 (<108) 不显示侧边栏
    let output = render_to_string(&state, &input, 80, 24);
    assert!(
        !output.contains("CONTEXT"),
        "narrow terminal should hide sidebar"
    );
}

#[test]
fn test_input_multiline_renders() {
    let state = AppState::new();
    let mut input = InputState::default();
    input.set_text("line one\nline two\nline three".to_string());
    let output = render_to_string(&state, &input, 80, 24);
    assert!(output.contains("line one"), "should render first line");
    assert!(output.contains("line two"), "should render second line");
}

#[test]
fn test_user_message_has_prefix_and_wraps() {
    let mut state = AppState::new();
    state.ui = serde_json::from_str(r#"{
        "appName": "rozsa",
        "version": "0.1.0",
        "cwd": "/home/user/project",
        "thinkingLevel": "medium",
        "isStreaming": false,
        "isCompacting": false,
        "messages": [
            {"role": "user", "content": [{"type": "text", "text": "Hello world, this is a longer message to test word wrapping behavior in the TUI"}]}
        ],
        "pendingMessages": [],
        "status": {},
        "widgetsAbove": {},
        "widgetsBelow": {},
        "keybindings": {"tui.input.submit": ["enter"]}
    }"#).unwrap();
    let input = InputState::default();
    let output = render_to_string(&state, &input, 50, 12);
    assert!(output.contains("›"), "should have › prefix on first line");
    assert!(output.contains("Hello world"), "should show message text");
    // 在 50 宽窄终端中消息应该被 wrap 到多行
    assert!(
        output.contains("wrapping behavior"),
        "long message should word-wrap to next line"
    );
}

#[test]
fn test_non_bash_tool_call_box_style() {
    let mut state = AppState::new();
    state.ui = default_ui_state();
    state.ui.messages = serde_json::from_str(r#"[
        {"role": "assistant", "content": [
            {"type": "toolCall", "name": "Read", "arguments": {"file_path": "/src/main.rs"}},
            {"type": "toolResult", "content": [{"type": "text", "text": "fn main() {\n    println!(\"hello\");\n}"}], "isError": false}
        ]}
    ]"#).unwrap();
    let input = InputState::default();
    let output = render_to_string(&state, &input, 60, 15);
    assert!(output.contains("Read"), "should show tool name");
    assert!(
        output.contains("/src/main.rs"),
        "should show file path preview"
    );
}

#[test]
fn test_streaming_status_position() {
    let mut state = AppState::new();
    state.ui = default_ui_state();
    state.ui.is_streaming = true;
    state.ui.messages = serde_json::from_str(
        r#"[
        {"role": "user", "content": [{"type": "text", "text": "hello"}]},
        {"role": "assistant", "content": [{"type": "text", "text": "I'm working on th"}]}
    ]"#,
    )
    .unwrap();
    let input = InputState::default();
    let output = render_to_string(&state, &input, 60, 15);
    std::fs::write("/tmp/tui_streaming_debug.txt", &output).unwrap();
    // assistant 的部分文本应该显示
    assert!(
        output.contains("I'm working on th"),
        "should show partial assistant text"
    );
    // spinner + Working... 贴在输入框上方
    assert!(
        output.contains("Working..."),
        "should show working indicator"
    );
}

#[test]
fn test_bash_tool_box_style_dump() {
    let mut state = AppState::new();
    state.ui = default_ui_state();
    state.ui.messages = serde_json::from_str(r#"[
        {"role": "assistant", "content": [
            {"type": "toolCall", "name": "bash", "arguments": {"command": "ls -la"}},
            {"type": "toolResult", "content": [{"type": "text", "text": "total 8\ndrwxr-xr-x 2 user user 4096 file1.txt\n-rw-r--r-- 1 user user  100 file2.rs"}], "isError": false}
        ]}
    ]"#).unwrap();
    let input = InputState::default();
    let output = render_to_string(&state, &input, 60, 18);
    assert!(output.contains("$ ls -la"), "should show command");
    assert!(output.contains("file1.txt"), "should show output");
    // 消息区域不应有整行 ─ 边框（输入框的 ╭──╮ 不算）
    let message_area: String = output.lines().take(10).collect::<Vec<_>>().join("\n");
    assert!(
        !message_area.contains("─"),
        "message area should NOT have ─ borders"
    );
}

#[test]
fn test_tool_result_error_line_fills_full_row_background() {
    let mut state = AppState::new();
    state.ui = default_ui_state();
    state.ui.messages = serde_json::from_str(r#"[
        {"role": "assistant", "content": [
            {"type": "toolCall", "name": "bash", "arguments": {"command": "false"}},
            {"type": "toolResult", "content": [{"type": "text", "text": "permission denied"}], "isError": true}
        ]}
    ]"#).unwrap();
    let input = InputState::default();
    let buf = render_to_buffer(&state, &input, 60, 18);
    let output = buffer_to_string(&buf);
    let error_y = output
        .lines()
        .position(|line| line.contains("(error)"))
        .expect("error marker row should render") as u16;

    for x in 0..60 {
        assert_eq!(
            buf.cell((x, error_y)).unwrap().bg,
            THEME.tool_pending_bg,
            "error marker row should keep tool result background at x={x}",
        );
    }
}

#[test]
fn test_tool_result_ansi_line_fills_full_row_background() {
    let mut state = AppState::new();
    state.ui = default_ui_state();
    state.ui.messages = serde_json::from_str(r#"[
        {"role": "assistant", "content": [
            {"type": "toolCall", "name": "bash", "arguments": {"command": "printf color"}},
            {"type": "toolResult", "content": [{"type": "text", "text": "\u001b[31mred\u001b[0m output"}], "isError": false}
        ]}
    ]"#).unwrap();
    let input = InputState::default();
    let buf = render_to_buffer(&state, &input, 60, 18);
    let output = buffer_to_string(&buf);
    let result_y = output
        .lines()
        .position(|line| line.contains("red output"))
        .expect("ansi result row should render") as u16;

    for x in 0..60 {
        assert_eq!(
            buf.cell((x, result_y)).unwrap().bg,
            THEME.tool_pending_bg,
            "ansi result row should keep tool result background at x={x}",
        );
    }
}

#[test]
fn test_assistant_text_wraps_with_sidebar_visible() {
    let mut state = AppState::new();
    state.ui = default_ui_state();
    state.ui.messages = serde_json::from_str(r#"[
        {"role": "assistant", "content": [{"type": "text", "text": "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi omicron pi rho sigma tau omega"}]}
    ]"#).unwrap();
    let input = InputState::default();
    let output = render_to_string(&state, &input, 120, 24);

    assert!(
        output.contains("alpha beta gamma"),
        "assistant text should render"
    );
    assert!(
        output.contains("tau omega"),
        "assistant text should wrap instead of clipping at the sidebar boundary"
    );
}

#[test]
fn test_input_border_uses_ts_editor_muted_color() {
    let state = AppState::new();
    let input = InputState::default();
    let buf = render_to_buffer(&state, &input, 80, 20);
    let border_cell = (0..buf.area.height)
        .flat_map(|y| (0..buf.area.width).map(move |x| (x, y)))
        .find(|&(x, y)| buf.cell((x, y)).unwrap().symbol() == "╭")
        .expect("input border should render");

    assert_eq!(
        buf.cell(border_cell).unwrap().fg,
        THEME.border_muted,
        "native input border should match the TS editor muted border"
    );
}
