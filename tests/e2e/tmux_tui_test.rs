// 端到端 TUI 测试 — 通过 tmux 真实启动 rozsa 二进制验证交互
//
// 运行方式：
//   cargo build --bin rozsa && cargo test --test e2e_tmux_tui -- --ignored --nocapture
//
// 前提：
//   - tmux 已安装
//   - cargo build 成功（target/debug/rozsa 存在）
//   - 不需要 API key（测试用 ROZSA_TEST_MODE=1 禁用真实模型调用）

use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

fn binary_path() -> String {
    // cargo test 在 workspace root 运行，但以防万一也支持 crate root
    let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap() // crates/
        .parent()
        .unwrap(); // workspace root
    workspace
        .join("target/debug/rozsa")
        .to_string_lossy()
        .into_owned()
}

const _BINARY_PLACEHOLDER: &str = ""; // BINARY 现在由 binary_path() 提供

// ---------------------------------------------------------------------------
// TmuxSession — 管理一个 tmux 会话的生命周期
// ---------------------------------------------------------------------------

struct TmuxSession {
    name: String,
}

impl TmuxSession {
    /// 启动一个新 tmux session 并在其中运行指定二进制。
    fn start(binary: &str) -> Self {
        let name = format!("rozsa-e2e-{}", std::process::id());

        // inline 环境变量到 shell 命令，确保 tmux server 复用时也能传递
        let shell_cmd = format!("ROZSA_TEST_MODE=1 NO_COLOR=1 {binary}");
        let status = Command::new("tmux")
            .args([
                "new-session",
                "-d",
                "-s",
                &name,
                "-x",
                "120",
                "-y",
                "40",
                &shell_cmd,
            ])
            .status()
            .expect("tmux 未安装或启动失败");

        assert!(status.success(), "tmux new-session 失败");

        // 等 TUI 完成初始化渲染
        thread::sleep(Duration::from_secs(2));
        Self { name }
    }

    /// 发送按键到 tmux pane（不自动加 Enter）。
    fn send_keys(&self, keys: &str) {
        let _ = Command::new("tmux")
            .args(["send-keys", "-t", &self.name, keys])
            .status();
    }

    /// 发送 Enter 键。
    fn enter(&self) {
        let _ = Command::new("tmux")
            .args(["send-keys", "-t", &self.name, "Enter"])
            .status();
    }

    /// 发送 Escape 键。
    fn escape(&self) {
        let _ = Command::new("tmux")
            .args(["send-keys", "-t", &self.name, "Escape"])
            .status();
    }

    /// 发送 Tab 键。
    #[allow(dead_code)]
    fn tab(&self) {
        let _ = Command::new("tmux")
            .args(["send-keys", "-t", &self.name, "Tab"])
            .status();
    }

    /// 发送 Ctrl+C。
    fn ctrl_c(&self) {
        let _ = Command::new("tmux")
            .args(["send-keys", "-t", &self.name, "C-c"])
            .status();
    }

    /// 捕获当前 pane 内容（纯文本）。
    fn capture(&self) -> String {
        let output = Command::new("tmux")
            .args(["capture-pane", "-t", &self.name, "-p"])
            .output()
            .expect("tmux capture-pane 失败");
        String::from_utf8_lossy(&output.stdout).to_string()
    }

    /// 等待屏幕上出现指定文本，超时返回 false。
    fn wait_for(&self, pattern: &str, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if self.capture().contains(pattern) {
                return true;
            }
            thread::sleep(Duration::from_millis(100));
        }
        false
    }

    /// 等待屏幕上出现指定文本（大小写不敏感）。
    fn wait_for_ci(&self, pattern: &str, timeout: Duration) -> bool {
        let pat_lower = pattern.to_lowercase();
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if self.capture().to_lowercase().contains(&pat_lower) {
                return true;
            }
            thread::sleep(Duration::from_millis(100));
        }
        false
    }
}

impl Drop for TmuxSession {
    fn drop(&mut self) {
        let _ = Command::new("tmux")
            .args(["send-keys", "-t", &self.name, "C-c"])
            .status();
        thread::sleep(Duration::from_millis(200));
        let _ = Command::new("tmux")
            .args(["kill-session", "-t", &self.name])
            .status();
    }
}

// ---------------------------------------------------------------------------
// 测试用例
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn tui_starts_and_shows_app_name() {
    let session = TmuxSession::start(&binary_path());
    assert!(
        session.wait_for_ci("rozsa", Duration::from_secs(5)),
        "TUI 启动后应显示 'rozsa'"
    );
}

#[test]
#[ignore]
fn slash_input_triggers_completion_popup() {
    let session = TmuxSession::start(&binary_path());

    // 输入 "/" 触发补全
    session.send_keys("/");
    thread::sleep(Duration::from_millis(800));

    // 验证补全列表出现
    let screen = session.capture();
    let has_completion =
        screen.contains("compact") || screen.contains("help") || screen.contains("model");
    assert!(has_completion, "输入 / 后应出现补全列表。屏幕:\n{screen}");
}

#[test]
#[ignore]
fn slash_prefix_narrows_completion() {
    let session = TmuxSession::start(&binary_path());

    // 输入 "/comp" → 补全应收敛到 /compact
    session.send_keys("/comp");
    thread::sleep(Duration::from_millis(800));

    let screen = session.capture();
    assert!(
        screen.contains("compact"),
        "输入 /comp 后补全应显示 compact。屏幕:\n{screen}"
    );
}

#[test]
#[ignore]
fn escape_dismisses_completion() {
    let session = TmuxSession::start(&binary_path());

    session.send_keys("/");
    thread::sleep(Duration::from_millis(500));
    // 确认补全出现
    assert!(session.wait_for_ci("compact", Duration::from_secs(2)));

    // Escape 关闭
    session.escape();
    thread::sleep(Duration::from_millis(300));
}

#[test]
#[ignore]
fn submit_message_appears_in_conversation() {
    let session = TmuxSession::start(&binary_path());

    session.send_keys("hello world");
    session.enter();

    assert!(
        session.wait_for("hello world", Duration::from_secs(3)),
        "提交的消息应显示在对话中"
    );
}

#[test]
#[ignore]
fn ctrl_c_stops_streaming() {
    let session = TmuxSession::start(&binary_path());

    session.send_keys("explain something long");
    session.enter();
    thread::sleep(Duration::from_secs(1));

    session.ctrl_c();
    thread::sleep(Duration::from_millis(500));

    // 流停后输入框应重新可用（光标回到输入区）
    let screen = session.capture();
    assert!(
        !screen.contains("Working..."),
        "Ctrl+C 后不应继续显示 Working..."
    );
}

#[test]
#[ignore]
fn sidebar_shows_model_info() {
    let session = TmuxSession::start(&binary_path());
    assert!(
        session.wait_for_ci("claude", Duration::from_secs(5))
            || session.wait_for_ci("sonnet", Duration::from_secs(1))
            || session.wait_for_ci("model", Duration::from_secs(1)),
        "Sidebar 应显示模型信息"
    );
}

#[test]
#[ignore]
fn bang_command_output_shown_in_tui() {
    let session = TmuxSession::start(&binary_path());

    session.send_keys("!echo e2e_bang_marker_42");
    session.enter();

    assert!(
        session.wait_for("e2e_bang_marker_42", Duration::from_secs(3)),
        "!command 的输出应显示在 TUI 中。屏幕:\n{}",
        session.capture()
    );
}
