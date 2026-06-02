// backend/mod.rs — AgentBackend trait 定义 + BackendEvent
//
// 内部结构:
// backend/
// ├── mod.rs ........... trait 定义、事件枚举、类型
// ├── socket.rs ........ SocketBackend（过渡期，与 TS AgentSession 通信）
// └── mock.rs .......... MockBackend（测试用）
//
// 相关文档:
// - [SPEC Design](../../../../dev-doc/refactor/tui/SPEC.md#design)

pub mod mock;
pub mod socket;

use std::pin::Pin;

use async_trait::async_trait;
use futures_core::Stream;
use serde_json::Value;
use tokio::sync::mpsc;

use crate::{
    components::model_selector::ModelEntry,
    protocol::{NativeGraphNode, NativePermissionPrompt, NativeUiState},
    components::session_selector::SessionEntry,
};

// --- 类型定义 ---

pub type BackendResult<T> = Result<T, BackendError>;
pub type EventStream = Pin<Box<dyn Stream<Item = BackendEvent> + Send>>;

#[derive(Debug, Clone)]
pub enum BackendError {
    NotConnected,
    ConnectionLost,
    Protocol(String),
    Internal(String),
}

impl std::fmt::Display for BackendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotConnected => write!(f, "not connected to backend"),
            Self::ConnectionLost => write!(f, "connection to backend lost"),
            Self::Protocol(msg) => write!(f, "protocol error: {msg}"),
            Self::Internal(msg) => write!(f, "internal error: {msg}"),
        }
    }
}

impl std::error::Error for BackendError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Direction {
    Forward,
    Backward,
}

// --- BackendEvent：后端推送给前端的事件 ---

#[derive(Debug, Clone)]
pub enum BackendEvent {
    /// 全量 UI 状态更新
    State(NativeUiState),
    /// 对话框弹出
    Dialog {
        id: String,
        kind: String,
        title: String,
        message: Option<String>,
        options: Vec<String>,
        text: Option<String>,
        selected: Option<usize>,
    },
    /// 通知消息
    Notify { level: String, message: String },
    /// 终端标题
    SetTitle(String),
    /// 覆盖输入框内容
    SetInput(String),
    /// 自动补全结果
    Autocomplete {
        id: u64,
        prefix: String,
        items: Vec<crate::protocol::NativeAutocompleteItem>,
    },
    /// 权限审批请求
    Permission(NativePermissionPrompt),
    /// 会话历史图
    Graph(Vec<NativeGraphNode>),
    /// 会话列表
    Sessions {
        entries: Vec<SessionEntry>,
        current_session_path: String,
    },
    /// 会话删除结果
    SessionDeleted {
        path: String,
        method: String,
        error: Option<String>,
    },
    /// 模型列表
    Models(Vec<ModelEntry>),
    /// 重试倒计时
    Retry { seconds: u32, reason: String },
    /// Compacting 状态
    Compacting(bool),
    /// 后端请求关闭
    Shutdown,
    /// 连接已断开
    Disconnected,
}

// --- AgentBackend trait ---

#[async_trait]
pub trait AgentBackend: Send + Sync {
    // --- 对话 ---
    async fn submit(&self, text: &str, images: Vec<ImageData>) -> BackendResult<()>;
    async fn abort(&self) -> BackendResult<()>;
    async fn follow_up(&self, text: &str, images: Vec<ImageData>) -> BackendResult<()>;
    async fn steer(&self, text: &str, images: Vec<ImageData>) -> BackendResult<()>;

    // --- 模型管理 ---
    async fn list_models(&self) -> BackendResult<()>;
    async fn switch_model(&self, id: &str) -> BackendResult<()>;
    async fn cycle_model(&self, direction: Direction) -> BackendResult<()>;

    // --- 会话管理 ---
    async fn list_sessions(&self) -> BackendResult<()>;
    async fn switch_session(&self, path: &str) -> BackendResult<()>;
    async fn delete_session(&self, path: &str) -> BackendResult<()>;
    async fn rename_session(&self, path: &str, name: &str) -> BackendResult<()>;

    // --- 权限审批 ---
    async fn respond_permission(
        &self,
        id: &str,
        choice: &str,
        trust_key: Option<&str>,
    ) -> BackendResult<()>;

    // --- 工具/Shell ---
    async fn run_bash(&self, command: &str) -> BackendResult<()>;

    // --- 上下文/编辑 ---
    async fn compact(&self) -> BackendResult<()>;
    async fn cycle_edit_mode(&self) -> BackendResult<()>;
    async fn switch_agent(&self, id: &str) -> BackendResult<()>;

    // --- 对话框 ---
    async fn dialog_response(
        &self,
        id: &str,
        value: Option<&str>,
        confirmed: Option<bool>,
        cancelled: Option<bool>,
    ) -> BackendResult<()>;

    // --- 自动补全 ---
    async fn autocomplete_request(
        &self,
        text: &str,
        cursor: usize,
        force: bool,
    ) -> BackendResult<()>;

    // --- 设置 ---
    async fn update_setting(&self, key: &str, value: &str) -> BackendResult<()>;

    // --- 生命周期 ---
    async fn connect(&self) -> BackendResult<()>;
    async fn disconnect(&self) -> BackendResult<()>;
    async fn exit(&self) -> BackendResult<()>;

    // --- 事件流（后端主动推送） ---
    fn events(&self) -> mpsc::UnboundedReceiver<BackendEvent>;
}

// --- 公共类型 ---

#[derive(Debug, Clone)]
pub struct ImageData {
    pub data: String,
    pub mime_type: String,
}

impl ImageData {
    pub fn from_base64(data: String) -> Self {
        let mime_type = if data.starts_with("iVBOR") {
            "image/png"
        } else if data.starts_with("/9j/") {
            "image/jpeg"
        } else if data.starts_with("R0lGOD") {
            "image/gif"
        } else if data.starts_with("UklGR") {
            "image/webp"
        } else {
            "image/png"
        };
        Self {
            data,
            mime_type: mime_type.to_string(),
        }
    }
}
