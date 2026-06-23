// PTY 集成测试 — 在真伪终端中启动 TUI 二进制，验证端到端行为
//
// 步骤：
// 1. 创建 Unix socket，作为 mock TS agent
// 2. 在 PTY 中启动 rozsa-tui 二进制（设置 ROZSA_NATIVE_TUI_SOCKET 环境变量）
// 3. 通过 socket 发送 state 消息
// 4. 用 vt100 解析 PTY 输出，断言屏幕内容
// 5. 发送键盘事件，验证交互响应

#![cfg(unix)]

use std::{
    io::{BufRead, BufReader, Read, Write},
    net::Shutdown,
    os::unix::net::{UnixListener, UnixStream},
    path::PathBuf,
    thread,
    time::{Duration, Instant},
};

use portable_pty::{native_pty_system, CommandBuilder, PtySize};

fn tui_binary_path() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let debug_path = manifest_dir.join("crates/rozsa-tui/target/debug/rozsa-tui");
    if debug_path.exists() {
        return debug_path;
    }
    // fallback to workspace target dir
    manifest_dir
        .join("target/debug/rozsa-tui")
        .canonicalize()
        .unwrap_or(debug_path)
}

fn make_state_message(app_name: &str, model: &str) -> String {
    format!(
        r#"{{"type":"state","state":{{"appName":"{}","version":"0.1.0","cwd":"/tmp","thinkingLevel":"medium","isStreaming":false,"isCompacting":false,"messages":[],"pendingMessages":[],"status":{{}},"widgetsAbove":{{}},"widgetsBelow":{{}},"keybindings":{{"tui.input.submit":["enter"],"tui.select.cancel":["escape"]}},"model":{{"id":"{}","provider":"test"}},"contextUsage":{{"percent":25.0}}}}}}"#,
        app_name, model
    )
}

fn make_state_with_messages() -> String {
    r#"{"type":"state","state":{"appName":"pi","version":"0.1.0","cwd":"/tmp","thinkingLevel":"medium","isStreaming":false,"isCompacting":false,"messages":[{"role":"user","content":[{"type":"text","text":"What is 2+2?"}]},{"role":"assistant","content":[{"type":"text","text":"The answer is 4."}]}],"pendingMessages":[],"status":{},"widgetsAbove":{},"widgetsBelow":{},"keybindings":{"tui.input.submit":["enter"],"tui.select.cancel":["escape"]},"model":{"id":"claude-sonnet-4","provider":"anthropic"}}}"#.to_string()
}

struct PtyHarness {
    master: Box<dyn portable_pty::MasterPty + Send>,
    child: Box<dyn portable_pty::Child + Send + Sync>,
    writer: Box<dyn Write + Send>,
    parser: vt100::Parser,
    buffer: Vec<u8>,
}

impl PtyHarness {
    fn spawn(socket_path: &str) -> Self {
        let binary = tui_binary_path();
        assert!(binary.exists(), "TUI binary not found at {:?}", binary);

        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows: 30,
                cols: 120,
                pixel_width: 0,
                pixel_height: 0,
            })
            .expect("openpty failed");

        let mut cmd = CommandBuilder::new(&binary);
        cmd.env("ROZSA_NATIVE_TUI_SOCKET", socket_path);
        cmd.env("TERM", "xterm-256color");

        let child = pair.slave.spawn_command(cmd).expect("spawn failed");
        drop(pair.slave);

        let writer = pair.master.take_writer().expect("take_writer");
        let parser = vt100::Parser::new(30, 120, 0);

        Self {
            master: pair.master,
            child,
            writer,
            parser,
            buffer: Vec::new(),
        }
    }

    fn read_frame(&mut self, timeout: Duration) -> bool {
        let start = Instant::now();
        let mut reader = self.master.try_clone_reader().expect("clone reader");
        let mut tmp = [0u8; 4096];
        loop {
            if start.elapsed() > timeout {
                return false;
            }
            // non-blocking read attempt
            match reader.read(&mut tmp) {
                Ok(0) => return false,
                Ok(n) => {
                    self.parser.process(&tmp[..n]);
                    self.buffer.extend_from_slice(&tmp[..n]);
                    return true;
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(10));
                }
                Err(_) => return false,
            }
        }
    }

    fn wait_for_content(&mut self, needle: &str, timeout: Duration) -> bool {
        let start = Instant::now();
        let mut reader = self.master.try_clone_reader().expect("clone reader");
        let mut tmp = [0u8; 4096];
        loop {
            if start.elapsed() > timeout {
                return false;
            }
            if self.screen_text().contains(needle) {
                return true;
            }
            match reader.read(&mut tmp) {
                Ok(0) => return false,
                Ok(n) => self.parser.process(&tmp[..n]),
                Err(_) => {
                    thread::sleep(Duration::from_millis(10));
                }
            }
        }
    }

    fn screen_text(&self) -> String {
        self.parser.screen().contents()
    }

    fn send_key(&mut self, bytes: &[u8]) {
        self.writer.write_all(bytes).ok();
        self.writer.flush().ok();
    }

    fn kill(&mut self) {
        self.child.kill().ok();
    }
}

/// 验证 TUI 能启动并渲染 header
#[test]
fn pty_boot_and_render_header() {
    let socket_dir = std::env::temp_dir();
    let socket_path = socket_dir.join(format!("rozsa-test-{}.sock", std::process::id()));
    let socket_path_str = socket_path.to_str().unwrap().to_string();

    // 启动 mock socket server
    let _ = std::fs::remove_file(&socket_path);
    let listener = UnixListener::bind(&socket_path).expect("bind socket");

    // 在 PTY 中启动 TUI
    let mut harness = PtyHarness::spawn(&socket_path_str);

    // 接受 TUI 连接
    listener.set_nonblocking(true).ok();
    let start = Instant::now();
    let mut client: Option<UnixStream> = None;
    while start.elapsed() < Duration::from_secs(5) {
        match listener.accept() {
            Ok((stream, _)) => {
                client = Some(stream);
                break;
            }
            Err(_) => thread::sleep(Duration::from_millis(50)),
        }
    }
    let mut client = client.expect("TUI should connect to socket");

    // 发送 state 消息
    let msg = make_state_message("rozsa", "claude-sonnet-4");
    writeln!(client, "{}", msg).expect("send state");
    client.flush().ok();

    // 等待渲染并验证
    thread::sleep(Duration::from_millis(200));
    // header 已移除，app name 显示在侧边栏
    let found = harness.wait_for_content("PI", Duration::from_secs(3));

    // 清理
    harness.send_key(b"\x03"); // Ctrl-C
    thread::sleep(Duration::from_millis(100));
    harness.send_key(b"\x03"); // 双击退出
    thread::sleep(Duration::from_millis(100));
    harness.kill();
    let _ = std::fs::remove_file(&socket_path);

    assert!(found, "Screen should contain 'PI' in sidebar. Got:\n{}", harness.screen_text());
}

/// 验证发送 shutdown 消息后 TUI 退出
#[test]
fn pty_shutdown_message_exits() {
    let socket_dir = std::env::temp_dir();
    let socket_path = socket_dir.join(format!("rozsa-test-shutdown-{}.sock", std::process::id()));
    let socket_path_str = socket_path.to_str().unwrap().to_string();

    let _ = std::fs::remove_file(&socket_path);
    let listener = UnixListener::bind(&socket_path).expect("bind");
    let mut harness = PtyHarness::spawn(&socket_path_str);

    listener.set_nonblocking(true).ok();
    let start = Instant::now();
    let mut client: Option<UnixStream> = None;
    while start.elapsed() < Duration::from_secs(5) {
        match listener.accept() {
            Ok((stream, _)) => {
                client = Some(stream);
                break;
            }
            Err(_) => thread::sleep(Duration::from_millis(50)),
        }
    }
    let mut client = client.expect("TUI should connect");

    // 先发 state 让 TUI 正常渲染
    let msg = make_state_message("rozsa", "test-model");
    writeln!(client, "{}", msg).ok();
    client.flush().ok();
    thread::sleep(Duration::from_millis(300));

    // 发送 shutdown
    writeln!(client, r#"{{"type":"shutdown"}}"#).ok();
    client.flush().ok();

    // 等待进程退出
    let exit_start = Instant::now();
    let mut exited = false;
    while exit_start.elapsed() < Duration::from_secs(3) {
        if let Ok(Some(_status)) = harness.child.try_wait() {
            exited = true;
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }

    if !exited {
        harness.kill();
    }
    let _ = std::fs::remove_file(&socket_path);
    assert!(exited, "TUI should exit after shutdown message");
}

/// 验证按键输入后 TUI 通过 socket 发送消息
#[test]
fn pty_input_sends_message() {
    let socket_dir = std::env::temp_dir();
    let socket_path = socket_dir.join(format!("rozsa-test-input-{}.sock", std::process::id()));
    let socket_path_str = socket_path.to_str().unwrap().to_string();

    let _ = std::fs::remove_file(&socket_path);
    let listener = UnixListener::bind(&socket_path).expect("bind");
    let mut harness = PtyHarness::spawn(&socket_path_str);

    listener.set_nonblocking(true).ok();
    let start = Instant::now();
    let mut client: Option<UnixStream> = None;
    while start.elapsed() < Duration::from_secs(5) {
        match listener.accept() {
            Ok((stream, _)) => {
                client = Some(stream);
                break;
            }
            Err(_) => thread::sleep(Duration::from_millis(50)),
        }
    }
    let mut client = client.expect("TUI should connect");
    client.set_read_timeout(Some(Duration::from_secs(3))).ok();

    // 发送初始 state
    let msg = make_state_message("rozsa", "test-model");
    writeln!(client, "{}", msg).ok();
    client.flush().ok();
    thread::sleep(Duration::from_millis(300));

    // 输入 "hello" + Enter
    harness.send_key(b"hello");
    thread::sleep(Duration::from_millis(50));
    harness.send_key(b"\r"); // Enter
    thread::sleep(Duration::from_millis(200));

    // 从 socket 读取 TUI 发来的消息
    let mut reader = BufReader::new(client.try_clone().unwrap());
    let mut received = String::new();
    let read_start = Instant::now();
    while read_start.elapsed() < Duration::from_secs(2) {
        match reader.read_line(&mut received) {
            Ok(0) => break,
            Ok(_) => break,
            Err(_) => {
                thread::sleep(Duration::from_millis(50));
            }
        }
    }

    // 清理
    writeln!(client, r#"{{"type":"shutdown"}}"#).ok();
    thread::sleep(Duration::from_millis(200));
    harness.kill();
    let _ = std::fs::remove_file(&socket_path);

    assert!(
        received.contains("hello"),
        "TUI should send 'hello' through socket. Got: {:?}",
        received
    );
}

/// 从 socket reader 中读取所有行，跳过 autocomplete_request 消息
fn read_non_autocomplete_lines(client: &UnixStream, timeout: Duration) -> Vec<String> {
    let mut reader = BufReader::new(client.try_clone().unwrap());
    let mut lines = Vec::new();
    let start = Instant::now();
    while start.elapsed() < timeout {
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {
                if !line.contains("autocomplete_request") {
                    lines.push(line);
                }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock
                || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                break;
            }
            Err(_) => break,
        }
    }
    lines
}

/// 验证 /help 作为 submit 发送到后端（过渡期：所有 slash 命令由 TS 后端处理）
#[test]
fn pty_slash_help_submits_to_backend() {
    let socket_dir = std::env::temp_dir();
    let socket_path = socket_dir.join(format!("rozsa-test-help-{}.sock", std::process::id()));
    let socket_path_str = socket_path.to_str().unwrap().to_string();

    let _ = std::fs::remove_file(&socket_path);
    let listener = UnixListener::bind(&socket_path).expect("bind");
    let mut harness = PtyHarness::spawn(&socket_path_str);

    listener.set_nonblocking(true).ok();
    let start = Instant::now();
    let mut client: Option<UnixStream> = None;
    while start.elapsed() < Duration::from_secs(5) {
        match listener.accept() {
            Ok((stream, _)) => {
                client = Some(stream);
                break;
            }
            Err(_) => thread::sleep(Duration::from_millis(50)),
        }
    }
    let mut client = client.expect("TUI should connect");
    client.set_read_timeout(Some(Duration::from_secs(2))).ok();

    // 发送初始 state
    let msg = make_state_message("rozsa", "test-model");
    writeln!(client, "{}", msg).ok();
    client.flush().ok();
    thread::sleep(Duration::from_millis(300));

    // 输入 /help + Enter
    harness.send_key(b"/help");
    thread::sleep(Duration::from_millis(100));
    harness.send_key(b"\r");
    thread::sleep(Duration::from_millis(500));

    // 读取非 autocomplete 消息 — 应收到 submit 含 "/help"
    let lines = read_non_autocomplete_lines(&client, Duration::from_secs(1));

    // 清理
    writeln!(client, r#"{{"type":"shutdown"}}"#).ok();
    thread::sleep(Duration::from_millis(200));
    harness.kill();
    let _ = std::fs::remove_file(&socket_path);

    let has_submit = lines.iter().any(|l| l.contains("submit") && l.contains("/help"));
    assert!(
        has_submit,
        "/help should be sent as submit to backend. Got: {:?}",
        lines
    );
}

/// 验证 /model 作为 submit 发送到后端
#[test]
fn pty_slash_model_submits_to_backend() {
    let socket_dir = std::env::temp_dir();
    let socket_path = socket_dir.join(format!("rozsa-test-model-{}.sock", std::process::id()));
    let socket_path_str = socket_path.to_str().unwrap().to_string();

    let _ = std::fs::remove_file(&socket_path);
    let listener = UnixListener::bind(&socket_path).expect("bind");
    let mut harness = PtyHarness::spawn(&socket_path_str);

    listener.set_nonblocking(true).ok();
    let start = Instant::now();
    let mut client: Option<UnixStream> = None;
    while start.elapsed() < Duration::from_secs(5) {
        match listener.accept() {
            Ok((stream, _)) => {
                client = Some(stream);
                break;
            }
            Err(_) => thread::sleep(Duration::from_millis(50)),
        }
    }
    let mut client = client.expect("TUI should connect");
    client.set_read_timeout(Some(Duration::from_secs(2))).ok();

    // 发送初始 state
    let msg = make_state_message("rozsa", "test-model");
    writeln!(client, "{}", msg).ok();
    client.flush().ok();
    thread::sleep(Duration::from_millis(300));

    // 输入 /model + Enter
    harness.send_key(b"/model");
    thread::sleep(Duration::from_millis(100));
    harness.send_key(b"\r");
    thread::sleep(Duration::from_millis(500));

    // 读取非 autocomplete 消息 — 应收到 submit 含 "/model"
    let lines = read_non_autocomplete_lines(&client, Duration::from_secs(1));

    // 清理
    writeln!(client, r#"{{"type":"shutdown"}}"#).ok();
    thread::sleep(Duration::from_millis(200));
    harness.kill();
    let _ = std::fs::remove_file(&socket_path);

    let has_submit = lines.iter().any(|l| l.contains("submit") && l.contains("/model"));
    assert!(
        has_submit,
        "/model should be sent as submit to backend. Got: {:?}",
        lines
    );
}

/// 验证 ROZSA_TUI_MODE=legacy 时立即退出
#[test]
fn pty_legacy_mode_exits_immediately() {
    let binary = tui_binary_path();
    assert!(binary.exists(), "TUI binary not found at {:?}", binary);

    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .expect("openpty failed");

    let mut cmd = CommandBuilder::new(&binary);
    cmd.env("ROZSA_TUI_MODE", "legacy");
    cmd.env("TERM", "xterm-256color");

    let mut child = pair.slave.spawn_command(cmd).expect("spawn failed");
    drop(pair.slave);

    // 应该立即退出（不需要 socket）
    let exit_start = Instant::now();
    let mut exited = false;
    while exit_start.elapsed() < Duration::from_secs(3) {
        if let Ok(Some(status)) = child.try_wait() {
            exited = true;
            assert!(
                status.success(),
                "ROZSA_TUI_MODE=legacy should exit with code 0"
            );
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }

    if !exited {
        child.kill().ok();
    }
    assert!(exited, "ROZSA_TUI_MODE=legacy should exit immediately");
}

/// 验证 Tab 补全后文本保留 / 前缀（如输入 /hel → Tab → /help ）
#[test]
fn pty_tab_completion_preserves_slash_prefix() {
    let socket_dir = std::env::temp_dir();
    let socket_path = socket_dir.join(format!("rozsa-test-tab-{}.sock", std::process::id()));
    let socket_path_str = socket_path.to_str().unwrap().to_string();

    let _ = std::fs::remove_file(&socket_path);
    let listener = UnixListener::bind(&socket_path).expect("bind");
    let mut harness = PtyHarness::spawn(&socket_path_str);

    listener.set_nonblocking(true).ok();
    let start = Instant::now();
    let mut client: Option<UnixStream> = None;
    while start.elapsed() < Duration::from_secs(5) {
        match listener.accept() {
            Ok((stream, _)) => {
                client = Some(stream);
                break;
            }
            Err(_) => thread::sleep(Duration::from_millis(50)),
        }
    }
    let mut client = client.expect("TUI should connect");
    client.set_read_timeout(Some(Duration::from_secs(2))).ok();

    // 发送初始 state
    let msg = make_state_message("rozsa", "test-model");
    writeln!(client, "{}", msg).ok();
    client.flush().ok();
    thread::sleep(Duration::from_millis(300));

    // 输入 /hel 触发 autocomplete
    harness.send_key(b"/hel");
    thread::sleep(Duration::from_millis(200));

    // 读掉 autocomplete_request
    let mut buf_reader = BufReader::new(client.try_clone().unwrap());
    let mut _discard = String::new();
    let _ = buf_reader.read_line(&mut _discard); // autocomplete for /
    let _ = buf_reader.read_line(&mut _discard); // autocomplete for /h
    let _ = buf_reader.read_line(&mut _discard); // autocomplete for /he
    let _ = buf_reader.read_line(&mut _discard); // autocomplete for /hel

    // 发送 autocomplete 响应（模拟 TS 后端返回 help 选项）
    let ac_response = r#"{"type":"autocomplete","id":4,"prefix":"/hel","items":[{"value":"help","label":"help","description":"Show help"}]}"#;
    writeln!(client, "{}", ac_response).ok();
    client.flush().ok();
    thread::sleep(Duration::from_millis(200));

    // 按 Tab 应用补全
    harness.send_key(b"\t");
    thread::sleep(Duration::from_millis(200));

    // 按 Enter 提交
    harness.send_key(b"\r");
    thread::sleep(Duration::from_millis(300));

    // 读取 submit 消息（跳过 autocomplete_request）
    let lines = read_non_autocomplete_lines(&client, Duration::from_secs(1));

    // 清理
    writeln!(client, r#"{{"type":"shutdown"}}"#).ok();
    thread::sleep(Duration::from_millis(200));
    harness.kill();
    let _ = std::fs::remove_file(&socket_path);

    // 验证提交的文本是 "/help "（包含 / 前缀）
    let has_slash_help = lines.iter().any(|l| l.contains("submit") && l.contains("/help"));
    assert!(
        has_slash_help,
        "After Tab completion, submit should contain '/help'. Got: {:?}",
        lines
    );
}

/// 验证 @ 输入触发 autocomplete_request
#[test]
fn pty_at_triggers_autocomplete_request() {
    let socket_dir = std::env::temp_dir();
    let socket_path = socket_dir.join(format!("rozsa-test-at-{}.sock", std::process::id()));
    let socket_path_str = socket_path.to_str().unwrap().to_string();

    let _ = std::fs::remove_file(&socket_path);
    let listener = UnixListener::bind(&socket_path).expect("bind");
    let mut harness = PtyHarness::spawn(&socket_path_str);

    listener.set_nonblocking(true).ok();
    let start = Instant::now();
    let mut client: Option<UnixStream> = None;
    while start.elapsed() < Duration::from_secs(5) {
        match listener.accept() {
            Ok((stream, _)) => { client = Some(stream); break; }
            Err(_) => thread::sleep(Duration::from_millis(50)),
        }
    }
    let mut client = client.expect("TUI should connect");
    client.set_read_timeout(Some(Duration::from_secs(2))).ok();

    let msg = make_state_message("rozsa", "test-model");
    writeln!(client, "{}", msg).ok();
    client.flush().ok();
    thread::sleep(Duration::from_millis(300));

    // 输入 @sr
    harness.send_key(b"@sr");
    thread::sleep(Duration::from_millis(300));

    // 读取消息
    let mut reader = BufReader::new(client.try_clone().unwrap());
    let mut lines = Vec::new();
    let read_start = Instant::now();
    while read_start.elapsed() < Duration::from_secs(1) {
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => lines.push(line),
            Err(_) => break,
        }
    }

    // 清理
    writeln!(client, r#"{{"type":"shutdown"}}"#).ok();
    thread::sleep(Duration::from_millis(200));
    harness.kill();
    let _ = std::fs::remove_file(&socket_path);

    let has_at = lines.iter().any(|l| l.contains("autocomplete_request") && l.contains("@"));
    assert!(
        has_at,
        "@ should trigger autocomplete_request with @ in text. Got: {:?}",
        lines
    );
}

/// 验证消息渲染正确（发送 state 后屏幕显示对话内容）
#[test]
fn pty_messages_render_correctly() {
    let socket_dir = std::env::temp_dir();
    let socket_path = socket_dir.join(format!("rozsa-test-msgs-{}.sock", std::process::id()));
    let socket_path_str = socket_path.to_str().unwrap().to_string();

    let _ = std::fs::remove_file(&socket_path);
    let listener = UnixListener::bind(&socket_path).expect("bind");
    let mut harness = PtyHarness::spawn(&socket_path_str);

    listener.set_nonblocking(true).ok();
    let start = Instant::now();
    let mut client: Option<UnixStream> = None;
    while start.elapsed() < Duration::from_secs(5) {
        match listener.accept() {
            Ok((stream, _)) => {
                client = Some(stream);
                break;
            }
            Err(_) => thread::sleep(Duration::from_millis(50)),
        }
    }
    let mut client = client.expect("TUI should connect");

    // 发送带消息的 state
    let msg = make_state_with_messages();
    writeln!(client, "{}", msg).ok();
    client.flush().ok();

    // 验证用户消息和回复都渲染到屏幕
    let found_user = harness.wait_for_content("What is 2+2", Duration::from_secs(3));
    let screen = harness.screen_text();
    let found_assistant = screen.contains("The answer is 4");

    // 清理
    writeln!(client, r#"{{"type":"shutdown"}}"#).ok();
    thread::sleep(Duration::from_millis(200));
    harness.kill();
    let _ = std::fs::remove_file(&socket_path);

    assert!(found_user, "User message should render. Screen:\n{}", screen);
    assert!(found_assistant, "Assistant reply should render. Screen:\n{}", screen);
}
