// rozsa-tui — Rust native TUI frontend
//
// Internal Framework:
// main.rs
// ├── app             — 应用主循环 (event loop + state)
// ├── ui/             — UI 渲染 (layout + render dispatch)
// ├── input/          — 输入处理 (keyboard + mouse + paste)
// ├── components/     — UI 组件 (editor, sidebar, selectors, permission...)
// ├── backend/        — 后端通信抽象 (socket/mock)
// ├── command/        — 命令系统
// ├── protocol        — 协议类型定义
// ├── overlay         — Overlay 定位与焦点
// ├── keymap          — 快捷键绑定匹配
// ├── theme/          — 颜色主题
// ├── markdown        — Markdown 渲染
// ├── highlight       — 代码语法高亮
// ├── hyperlink       — OSC 8 终端超链接
// ├── terminal_image  — 终端图片协议
// ├── terminal_caps   — 终端能力检测
// ├── ansi            — ANSI SGR 解析
// ├── fuzzy           — Fuzzy 匹配
// ├── undo            — Undo 栈
// └── kill_ring       — Kill Ring
//
// Related Docs:
// - [TUI Design](../../docs/rozsa_framework.md#rozsa-tui--ratatui-终端前端)
// - [Protocol](../../packages/coding-agent/src/modes/native/protocol.ts)

mod ansi;
mod app;
#[allow(dead_code, unused_imports, clippy::large_enum_variant)]
mod backend;
#[allow(dead_code)]
mod command;
mod components;
mod fuzzy;
mod highlight;
mod hyperlink;
mod input;
mod keymap;
mod kill_ring;
mod markdown;
mod overlay;
mod protocol;
mod terminal_caps;
mod terminal_image;
mod theme;
mod ui;
mod undo;

/// 统一换行符：\r\n 和 \r 归一化为 \n
fn normalize_newlines(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // ROZSA_TUI_MODE=legacy 时退出，让 TS 前端接管
    if std::env::var("ROZSA_TUI_MODE").as_deref() == Ok("legacy") {
        return Ok(());
    }
    app::run().await
}

#[cfg(test)]
mod tests {
    use crate::protocol::HostMessage;

    #[test]
    fn test_state_deserialization() {
        let json = r#"{"type":"state","state":{"appName":"pi","version":"0.1.0","cwd":"/tmp","sessionName":"test","model":{"id":"claude-sonnet-4-20250514","provider":"anthropic","name":"Claude Sonnet","api":"anthropic","baseUrl":"https://api.anthropic.com","reasoning":true,"input":["text","image"],"cost":{"input":3,"output":15}},"thinkingLevel":"medium","isStreaming":false,"isCompacting":false,"messages":[{"role":"user","content":[{"type":"text","text":"hello"}]},{"role":"assistant","content":[{"type":"thinking","thinking":"let me think..."},{"type":"text","text":"Hi there!"}]}],"pendingMessages":["queued msg"],"status":{"edit":"normal"},"widgetsAbove":{},"widgetsBelow":{},"stats":{"tokens":{"total":100},"cost":0.001},"runtimeState":{"activeSubagents":[],"editMode":"normal","permission":{"mode":"on-request"},"modelUsage":{"model":"claude-sonnet-4-20250514","promptTokens":50,"completionTokens":50,"sessionTotalTokens":100},"gitStatus":{"enabled":true,"branch":"main","uncommittedChangesCount":2}},"contextUsage":{"tokens":50000,"contextWindow":200000,"percent":25.0},"keybindings":{"tui.input.submit":["enter"],"tui.select.cancel":["escape"]},"error":null}}"#;
        let msg: HostMessage = serde_json::from_str(json).expect("state deserialization failed");
        match msg {
            HostMessage::State { state } => {
                assert_eq!(state.app_name, "pi");
                assert_eq!(state.version, "0.1.0");
                assert_eq!(state.model.as_ref().unwrap().id, "claude-sonnet-4-20250514");
                assert_eq!(state.model.as_ref().unwrap().provider, "anthropic");
                assert_eq!(state.thinking_level, "medium");
                assert!(!state.is_streaming);
                assert_eq!(state.messages.len(), 2);
                assert_eq!(state.pending_messages.len(), 1);
            }
            _ => panic!("expected State variant"),
        }
    }

    #[test]
    fn test_dialog_deserialization() {
        let json = r#"{"type":"dialog","id":"dlg-1","kind":"select","title":"Choose","message":"Pick one","options":["a","b","c"]}"#;
        let msg: HostMessage = serde_json::from_str(json).expect("dialog deserialization failed");
        match msg {
            HostMessage::Dialog { id, kind, title, options, .. } => {
                assert_eq!(id, "dlg-1");
                assert_eq!(kind, "select");
                assert_eq!(title, "Choose");
                assert_eq!(options.unwrap().len(), 3);
            }
            _ => panic!("expected Dialog variant"),
        }
    }

    #[test]
    fn test_permission_deserialization() {
        let json = r#"{"type":"permission","prompt":{"id":"perm-1","request":{"toolName":"bash","command":"rm -rf /"},"context":{"riskLevel":"high"},"trustLevels":[{"label":"This session","key":"session"},{"label":"Always","key":"always"}]}}"#;
        let msg: HostMessage = serde_json::from_str(json).expect("permission deserialization failed");
        match msg {
            HostMessage::Permission { prompt } => {
                assert_eq!(prompt.id, "perm-1");
                assert_eq!(prompt.trust_levels.len(), 2);
            }
            _ => panic!("expected Permission variant"),
        }
    }

    #[test]
    fn test_shutdown_deserialization() {
        let json = r#"{"type":"shutdown"}"#;
        let msg: HostMessage = serde_json::from_str(json).expect("shutdown deserialization failed");
        assert!(matches!(msg, HostMessage::Shutdown));
    }

    #[test]
    fn test_unknown_type_fails_gracefully() {
        let json = r#"{"type":"unknown_future_message","data":123}"#;
        assert!(serde_json::from_str::<HostMessage>(json).is_err());
    }

    #[test]
    fn test_client_message_serialization() {
        use crate::protocol::{ClientMessage, ImagePayload};

        let msg = ClientMessage::Submit { text: "hello", images: vec![] };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""type":"submit""#));
        assert!(json.contains(r#""text":"hello""#));
        assert!(!json.contains("images"));

        let img = ImagePayload::from_base64("iVBORdata".to_string());
        let msg = ClientMessage::Submit { text: "hi", images: vec![img] };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("images"));
        assert!(json.contains(r#""mimeType":"image/png""#));
        assert!(json.contains(r#""data":"iVBORdata""#));

        let msg = ClientMessage::Abort;
        let json = serde_json::to_string(&msg).unwrap();
        assert_eq!(json, r#"{"type":"abort"}"#);

        let msg = ClientMessage::SwitchModel {
            provider: "nvidia",
            id: "google/gemma-4-31b-it",
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert_eq!(
            json,
            r#"{"type":"switch_model","provider":"nvidia","id":"google/gemma-4-31b-it"}"#
        );

        let msg = ClientMessage::PermissionResponse { id: "p1", choice: "approve_once", trust_key: None };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""id":"p1""#));
        assert!(!json.contains("trustKey"));
    }

    #[test]
    fn test_update_setting_serialization() {
        use crate::protocol::ClientMessage;

        let msg = ClientMessage::UpdateSetting { key: "thinkingLevel", value: "high" };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""type":"update_setting""#));
        assert!(json.contains(r#""key":"thinkingLevel""#));
        assert!(json.contains(r#""value":"high""#));
    }

    #[test]
    fn test_cycle_thinking_serialization() {
        use crate::protocol::ClientMessage;

        let msg = ClientMessage::CycleThinking;
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""type":"cycle_thinking""#));
    }
}
