// backend/mock.rs — MockBackend 实现（测试用）
//
// 内部结构:
// mock.rs
// ├── MockBackend          # 可预设响应的测试 backend
// └── MockBackendBuilder   # 构建器模式配置预设事件

use std::sync::Mutex;

use async_trait::async_trait;
use tokio::sync::mpsc;

use super::{AgentBackend, BackendError, BackendEvent, BackendResult, Direction, ImageData};

/// 测试用 Backend，支持预设事件序列
pub struct MockBackend {
    tx: mpsc::UnboundedSender<BackendEvent>,
    rx: Mutex<Option<mpsc::UnboundedReceiver<BackendEvent>>>,
    /// 预设的事件队列，connect 时一次性推送
    preset_events: Mutex<Vec<BackendEvent>>,
    /// 记录所有调用（供断言使用）
    calls: Mutex<Vec<MockCall>>,
}

#[derive(Debug, Clone)]
pub enum MockCall {
    Submit { text: String },
    Abort,
    FollowUp { text: String },
    Steer { text: String },
    ListModels,
    SwitchModel { provider: String, id: String },
    CycleModel { direction: Direction },
    ListSessions,
    SwitchSession { path: String },
    DeleteSession { path: String },
    RenameSession { path: String, name: String },
    RespondPermission { id: String, choice: String },
    RunBash { command: String },
    Compact,
    CycleEditMode,
    SwitchAgent { id: String },
    DialogResponse { id: String },
    AutocompleteRequest { text: String, cursor: usize },
    UpdateSetting { key: String, value: String },
    Connect,
    Disconnect,
    Exit,
}

impl Default for MockBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl MockBackend {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        Self {
            tx,
            rx: Mutex::new(Some(rx)),
            preset_events: Mutex::new(Vec::new()),
            calls: Mutex::new(Vec::new()),
        }
    }

    /// 添加预设事件（connect 后推送）
    pub fn with_events(self, events: Vec<BackendEvent>) -> Self {
        *self.preset_events.lock().unwrap() = events;
        self
    }

    /// 手动注入事件（在运行中推送）
    pub fn inject_event(&self, event: BackendEvent) {
        let _ = self.tx.send(event);
    }

    /// 获取所有调用记录
    pub fn calls(&self) -> Vec<MockCall> {
        self.calls.lock().unwrap().clone()
    }

    fn record(&self, call: MockCall) {
        self.calls.lock().unwrap().push(call);
    }
}

#[async_trait]
impl AgentBackend for MockBackend {
    async fn submit(&self, text: &str, _images: Vec<ImageData>) -> BackendResult<()> {
        self.record(MockCall::Submit {
            text: text.to_string(),
        });
        Ok(())
    }

    async fn abort(&self) -> BackendResult<()> {
        self.record(MockCall::Abort);
        Ok(())
    }

    async fn follow_up(&self, text: &str, _images: Vec<ImageData>) -> BackendResult<()> {
        self.record(MockCall::FollowUp {
            text: text.to_string(),
        });
        Ok(())
    }

    async fn steer(&self, text: &str, _images: Vec<ImageData>) -> BackendResult<()> {
        self.record(MockCall::Steer {
            text: text.to_string(),
        });
        Ok(())
    }

    async fn list_models(&self) -> BackendResult<()> {
        self.record(MockCall::ListModels);
        Ok(())
    }

    async fn switch_model(&self, provider: &str, id: &str) -> BackendResult<()> {
        self.record(MockCall::SwitchModel {
            provider: provider.to_string(),
            id: id.to_string(),
        });
        Ok(())
    }

    async fn cycle_model(&self, direction: Direction) -> BackendResult<()> {
        self.record(MockCall::CycleModel { direction });
        Ok(())
    }

    async fn list_sessions(&self) -> BackendResult<()> {
        self.record(MockCall::ListSessions);
        Ok(())
    }

    async fn switch_session(&self, path: &str) -> BackendResult<()> {
        self.record(MockCall::SwitchSession {
            path: path.to_string(),
        });
        Ok(())
    }

    async fn delete_session(&self, path: &str) -> BackendResult<()> {
        self.record(MockCall::DeleteSession {
            path: path.to_string(),
        });
        Ok(())
    }

    async fn rename_session(&self, path: &str, name: &str) -> BackendResult<()> {
        self.record(MockCall::RenameSession {
            path: path.to_string(),
            name: name.to_string(),
        });
        Ok(())
    }

    async fn respond_permission(
        &self,
        id: &str,
        choice: &str,
        _trust_key: Option<&str>,
    ) -> BackendResult<()> {
        self.record(MockCall::RespondPermission {
            id: id.to_string(),
            choice: choice.to_string(),
        });
        Ok(())
    }

    async fn run_bash(&self, command: &str) -> BackendResult<()> {
        self.record(MockCall::RunBash {
            command: command.to_string(),
        });
        Ok(())
    }

    async fn compact(&self) -> BackendResult<()> {
        self.record(MockCall::Compact);
        Ok(())
    }

    async fn cycle_edit_mode(&self) -> BackendResult<()> {
        self.record(MockCall::CycleEditMode);
        Ok(())
    }

    async fn switch_agent(&self, id: &str) -> BackendResult<()> {
        self.record(MockCall::SwitchAgent {
            id: id.to_string(),
        });
        Ok(())
    }

    async fn dialog_response(
        &self,
        id: &str,
        _value: Option<&str>,
        _confirmed: Option<bool>,
        _cancelled: Option<bool>,
    ) -> BackendResult<()> {
        self.record(MockCall::DialogResponse {
            id: id.to_string(),
        });
        Ok(())
    }

    async fn autocomplete_request(
        &self,
        text: &str,
        cursor: usize,
        _force: bool,
    ) -> BackendResult<()> {
        self.record(MockCall::AutocompleteRequest {
            text: text.to_string(),
            cursor,
        });
        Ok(())
    }

    async fn update_setting(&self, key: &str, value: &str) -> BackendResult<()> {
        self.record(MockCall::UpdateSetting {
            key: key.to_string(),
            value: value.to_string(),
        });
        Ok(())
    }

    async fn connect(&self) -> BackendResult<()> {
        self.record(MockCall::Connect);
        // 推送预设事件
        let events = std::mem::take(&mut *self.preset_events.lock().unwrap());
        for event in events {
            let _ = self.tx.send(event);
        }
        Ok(())
    }

    async fn disconnect(&self) -> BackendResult<()> {
        self.record(MockCall::Disconnect);
        Ok(())
    }

    async fn exit(&self) -> BackendResult<()> {
        self.record(MockCall::Exit);
        let _ = self.tx.send(BackendEvent::Shutdown);
        Ok(())
    }

    fn events(&self) -> mpsc::UnboundedReceiver<BackendEvent> {
        self.rx
            .lock()
            .unwrap()
            .take()
            .expect("events() can only be called once")
    }
}
