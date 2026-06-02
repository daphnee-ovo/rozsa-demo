// backend/socket.rs — SocketBackend（过渡期，复用现有 Unix socket 协议）
//
// 内部结构:
// socket.rs
// ├── SocketBackend       # AgentBackend 实现，通过 Unix socket 与 TS AgentSession 通信
// ├── connect()           # 异步连接 socket，启动读取 task
// └── send_message()      # 序列化 ClientMessage 并写入 socket

use std::{
    io::Write as _,
    os::unix::net::UnixStream as StdUnixStream,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use async_trait::async_trait;
use tokio::sync::mpsc;

use super::{AgentBackend, BackendError, BackendEvent, BackendResult, Direction, ImageData};
use crate::protocol::{ClientMessage, HostMessage, ImagePayload, send};

/// 过渡期 Backend：通过 Unix socket 与 TS AgentSession 通信
pub struct SocketBackend {
    socket_path: String,
    writer: Arc<Mutex<Option<StdUnixStream>>>,
    event_tx: mpsc::UnboundedSender<BackendEvent>,
    event_rx: Mutex<Option<mpsc::UnboundedReceiver<BackendEvent>>>,
}

impl SocketBackend {
    pub fn new(socket_path: String) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        Self {
            socket_path,
            writer: Arc::new(Mutex::new(None)),
            event_tx: tx,
            event_rx: Mutex::new(Some(rx)),
        }
    }

    /// 获取 writer 的共享引用（用于兼容现有 protocol::send）
    pub fn writer(&self) -> Option<Arc<Mutex<StdUnixStream>>> {
        let guard = self.writer.lock().unwrap();
        guard
            .as_ref()
            .map(|stream| Arc::new(Mutex::new(stream.try_clone().unwrap())))
    }

    fn send_msg(&self, msg: &ClientMessage<'_>) -> BackendResult<()> {
        let guard = self.writer.lock().unwrap();
        let Some(ref stream) = *guard else {
            return Err(BackendError::NotConnected);
        };
        // 直接在已锁的 stream 上写入，避免 try_clone 导致 fd 泄漏
        let json = serde_json::to_string(msg).map_err(|e| BackendError::Protocol(e.to_string()))?;
        let mut writer = stream
            .try_clone()
            .map_err(|e| BackendError::Internal(e.to_string()))?;
        writeln!(writer, "{json}").map_err(|e| BackendError::Internal(e.to_string()))
    }

    fn images_to_payload(images: Vec<ImageData>) -> Vec<ImagePayload> {
        images
            .into_iter()
            .map(|img| ImagePayload::from_base64(img.data))
            .collect()
    }
}

#[async_trait]
impl AgentBackend for SocketBackend {
    async fn submit(&self, text: &str, images: Vec<ImageData>) -> BackendResult<()> {
        self.send_msg(&ClientMessage::Submit {
            text,
            images: Self::images_to_payload(images),
        })
    }

    async fn abort(&self) -> BackendResult<()> {
        self.send_msg(&ClientMessage::Abort)
    }

    async fn follow_up(&self, text: &str, images: Vec<ImageData>) -> BackendResult<()> {
        self.send_msg(&ClientMessage::FollowUp {
            text,
            images: Self::images_to_payload(images),
        })
    }

    async fn steer(&self, text: &str, images: Vec<ImageData>) -> BackendResult<()> {
        self.send_msg(&ClientMessage::Steer {
            text,
            images: Self::images_to_payload(images),
        })
    }

    async fn list_models(&self) -> BackendResult<()> {
        self.send_msg(&ClientMessage::ListModels)
    }

    async fn switch_model(&self, id: &str) -> BackendResult<()> {
        self.send_msg(&ClientMessage::SwitchModel { id })
    }

    async fn cycle_model(&self, direction: Direction) -> BackendResult<()> {
        let dir = match direction {
            Direction::Forward => "forward",
            Direction::Backward => "backward",
        };
        self.send_msg(&ClientMessage::CycleModel { direction: dir })
    }

    async fn list_sessions(&self) -> BackendResult<()> {
        self.send_msg(&ClientMessage::ListSessions { scope: "current" })
    }

    async fn switch_session(&self, path: &str) -> BackendResult<()> {
        self.send_msg(&ClientMessage::SwitchSession { path })
    }

    async fn delete_session(&self, path: &str) -> BackendResult<()> {
        self.send_msg(&ClientMessage::DeleteSession { path })
    }

    async fn rename_session(&self, path: &str, name: &str) -> BackendResult<()> {
        self.send_msg(&ClientMessage::RenameSession { path, name })
    }

    async fn respond_permission(
        &self,
        id: &str,
        choice: &str,
        trust_key: Option<&str>,
    ) -> BackendResult<()> {
        self.send_msg(&ClientMessage::PermissionResponse {
            id,
            choice,
            trust_key,
        })
    }

    async fn run_bash(&self, command: &str) -> BackendResult<()> {
        self.send_msg(&ClientMessage::Bash { command })
    }

    async fn compact(&self) -> BackendResult<()> {
        self.send_msg(&ClientMessage::Compact)
    }

    async fn cycle_edit_mode(&self) -> BackendResult<()> {
        self.send_msg(&ClientMessage::CycleEditMode)
    }

    async fn switch_agent(&self, id: &str) -> BackendResult<()> {
        self.send_msg(&ClientMessage::SwitchAgent { id })
    }

    async fn dialog_response(
        &self,
        id: &str,
        value: Option<&str>,
        confirmed: Option<bool>,
        cancelled: Option<bool>,
    ) -> BackendResult<()> {
        self.send_msg(&ClientMessage::DialogResponse {
            id,
            value,
            confirmed,
            cancelled,
        })
    }

    async fn autocomplete_request(
        &self,
        text: &str,
        cursor: usize,
        force: bool,
    ) -> BackendResult<()> {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let id = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.send_msg(&ClientMessage::AutocompleteRequest {
            id,
            text,
            cursor,
            force,
        })
    }

    async fn update_setting(&self, key: &str, value: &str) -> BackendResult<()> {
        self.send_msg(&ClientMessage::UpdateSetting { key, value })
    }

    async fn connect(&self) -> BackendResult<()> {
        // 使用 tokio spawn_blocking 避免阻塞 async runtime
        let path = self.socket_path.clone();
        let stream = tokio::task::spawn_blocking(move || {
            let started = Instant::now();
            loop {
                match StdUnixStream::connect(&path) {
                    Ok(stream) => return Ok(stream),
                    Err(error)
                        if started.elapsed() < Duration::from_secs(10)
                            && matches!(
                                error.kind(),
                                std::io::ErrorKind::NotFound
                                    | std::io::ErrorKind::ConnectionRefused
                            ) =>
                    {
                        std::thread::sleep(Duration::from_millis(25));
                    }
                    Err(error) => return Err(error),
                }
            }
        })
        .await
        .map_err(|e| BackendError::Internal(format!("spawn_blocking failed: {e}")))?
        .map_err(|e| BackendError::Internal(format!("connect failed: {e}")))?;

        let reader_stream = stream
            .try_clone()
            .map_err(|e| BackendError::Internal(e.to_string()))?;

        *self.writer.lock().unwrap() = Some(stream);

        // 启动读取线程
        let tx = self.event_tx.clone();
        std::thread::spawn(move || {
            use std::io::BufRead;
            let reader = std::io::BufReader::new(reader_stream);
            for line in reader.lines() {
                match line {
                    Ok(line) => match serde_json::from_str::<HostMessage>(&line) {
                        Ok(msg) => {
                            let event = host_message_to_event(msg);
                            if tx.send(event).is_err() {
                                break;
                            }
                        }
                        Err(e) => {
                            tracing::warn!(
                                "protocol parse error: {e}, line: {}",
                                &line[..line.len().min(200)]
                            );
                            continue;
                        }
                    },
                    Err(e) => {
                        tracing::warn!("reader IO error: {e}");
                        break;
                    }
                }
            }
            let _ = tx.send(BackendEvent::Disconnected);
        });

        Ok(())
    }

    async fn disconnect(&self) -> BackendResult<()> {
        *self.writer.lock().unwrap() = None;
        Ok(())
    }

    async fn exit(&self) -> BackendResult<()> {
        self.send_msg(&ClientMessage::Exit)
    }

    fn events(&self) -> mpsc::UnboundedReceiver<BackendEvent> {
        self.event_rx
            .lock()
            .unwrap()
            .take()
            .expect("events() can only be called once")
    }
}

/// 将 HostMessage 转换为 BackendEvent
fn host_message_to_event(msg: HostMessage) -> BackendEvent {
    match msg {
        HostMessage::State { state } => BackendEvent::State(state),
        HostMessage::Dialog {
            id,
            kind,
            title,
            message,
            options,
            text,
            selected,
        } => BackendEvent::Dialog {
            id,
            kind,
            title,
            message,
            options: options.unwrap_or_default(),
            text,
            selected,
        },
        HostMessage::Notify { level, message } => BackendEvent::Notify { level, message },
        HostMessage::SetTitle { title } => BackendEvent::SetTitle(title),
        HostMessage::SetInput { text } => BackendEvent::SetInput(text),
        HostMessage::Autocomplete { id, prefix, items } => {
            BackendEvent::Autocomplete { id, prefix, items }
        }
        HostMessage::Permission { prompt } => BackendEvent::Permission(prompt),
        HostMessage::Graph { nodes } => BackendEvent::Graph(nodes),
        HostMessage::Sessions {
            entries,
            current_session_path,
        } => BackendEvent::Sessions {
            entries,
            current_session_path,
        },
        HostMessage::SessionDeleted {
            path,
            method,
            error,
        } => BackendEvent::SessionDeleted {
            path,
            method,
            error,
        },
        HostMessage::Models { entries } => BackendEvent::Models(entries),
        HostMessage::Retry { seconds, reason } => BackendEvent::Retry { seconds, reason },
        HostMessage::Compacting { active } => BackendEvent::Compacting(active),
        HostMessage::Shutdown => BackendEvent::Shutdown,
    }
}
