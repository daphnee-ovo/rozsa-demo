use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::{mpsc, Mutex};

use rozsa_app::agent_session::AgentSession;
use rozsa_core::events::AgentEvent;
use rozsa_core::messages::AgentMessage;
use rozsa_model::types::{ContentBlock, Message};

use crate::protocol::{ModelInfo, NativeUiState};

use super::{AgentBackend, BackendError, BackendEvent, BackendResult, Direction, ImageData};

pub struct NativeBackend {
    session: Arc<Mutex<AgentSession>>,
    event_tx: mpsc::UnboundedSender<BackendEvent>,
    event_rx: Mutex<Option<mpsc::UnboundedReceiver<BackendEvent>>>,
}

impl NativeBackend {
    pub fn new(session: AgentSession) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        Self {
            session: Arc::new(Mutex::new(session)),
            event_tx: tx,
            event_rx: Mutex::new(Some(rx)),
        }
    }

    fn push_state(&self, session: &AgentSession) {
        let messages: Vec<serde_json::Value> = session
            .messages()
            .iter()
            .filter_map(|msg| serde_json::to_value(msg).ok())
            .collect();

        let model = session.model();
        let state = NativeUiState {
            app_name: "rozsa".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            cwd: session.cwd().to_string_lossy().to_string(),
            session_name: None,
            model: Some(ModelInfo {
                id: model.id.clone(),
                provider: format!("{:?}", model.provider),
            }),
            thinking_level: format!("{:?}", session.thinking_level()),
            is_streaming: session.is_running(),
            is_compacting: false,
            hide_thinking: false,
            show_images: true,
            messages,
            pending_messages: vec![],
            status: BTreeMap::new(),
            widgets_above: BTreeMap::new(),
            widgets_below: BTreeMap::new(),
            stats: None,
            runtime_state: None,
            context_usage: None,
            keybindings: BTreeMap::new(),
            error: None,
        };

        let _ = self.event_tx.send(BackendEvent::State(state));
    }
}

#[async_trait]
impl AgentBackend for NativeBackend {
    async fn submit(&self, text: &str, _images: Vec<ImageData>) -> BackendResult<()> {
        let session = self.session.clone();
        let tx = self.event_tx.clone();
        let text = text.to_string();

        let session_ref = session.clone();
        tokio::spawn(async move {
            let mut sess = session_ref.lock().await;
            match sess.prompt(&text).await {
                Ok(_events) => {
                    // Push final state
                    let messages: Vec<serde_json::Value> = sess
                        .messages()
                        .iter()
                        .filter_map(|msg| serde_json::to_value(msg).ok())
                        .collect();

                    let model = sess.model();
                    let state = NativeUiState {
                        app_name: "rozsa".to_string(),
                        version: env!("CARGO_PKG_VERSION").to_string(),
                        cwd: sess.cwd().to_string_lossy().to_string(),
                        session_name: None,
                        model: Some(ModelInfo {
                            id: model.id.clone(),
                            provider: format!("{:?}", model.provider),
                        }),
                        thinking_level: format!("{:?}", sess.thinking_level()),
                        is_streaming: false,
                        is_compacting: false,
                        hide_thinking: false,
                        show_images: true,
                        messages,
                        pending_messages: vec![],
                        status: BTreeMap::new(),
                        widgets_above: BTreeMap::new(),
                        widgets_below: BTreeMap::new(),
                        stats: None,
                        runtime_state: None,
                        context_usage: None,
                        keybindings: BTreeMap::new(),
                        error: None,
                    };
                    let _ = tx.send(BackendEvent::State(state));
                }
                Err(e) => {
                    let _ = tx.send(BackendEvent::Notify {
                        level: "error".to_string(),
                        message: e.to_string(),
                    });
                }
            }
        });

        Ok(())
    }

    async fn abort(&self) -> BackendResult<()> {
        let mut sess = self.session.lock().await;
        sess.abort();
        Ok(())
    }

    async fn follow_up(&self, text: &str, _images: Vec<ImageData>) -> BackendResult<()> {
        // Queue as follow-up, will be picked up on next turn
        self.submit(text, vec![]).await
    }

    async fn steer(&self, text: &str, _images: Vec<ImageData>) -> BackendResult<()> {
        // Queue as steering message
        self.submit(text, vec![]).await
    }

    async fn list_models(&self) -> BackendResult<()> {
        // TODO: populate from model registry
        let _ = self.event_tx.send(BackendEvent::Models(vec![]));
        Ok(())
    }

    async fn switch_model(&self, _provider: &str, _id: &str) -> BackendResult<()> {
        // TODO: resolve model from registry and call session.set_model()
        Ok(())
    }

    async fn cycle_model(&self, _direction: Direction) -> BackendResult<()> {
        Ok(())
    }

    async fn list_sessions(&self) -> BackendResult<()> {
        let _ = self.event_tx.send(BackendEvent::Sessions {
            entries: vec![],
            current_session_path: String::new(),
        });
        Ok(())
    }

    async fn switch_session(&self, _path: &str) -> BackendResult<()> {
        Ok(())
    }

    async fn delete_session(&self, _path: &str) -> BackendResult<()> {
        Ok(())
    }

    async fn rename_session(&self, _path: &str, _name: &str) -> BackendResult<()> {
        Ok(())
    }

    async fn respond_permission(
        &self,
        _id: &str,
        _choice: &str,
        _trust_key: Option<&str>,
    ) -> BackendResult<()> {
        Ok(())
    }

    async fn run_bash(&self, command: &str) -> BackendResult<()> {
        self.submit(&format!("!{command}"), vec![]).await
    }

    async fn compact(&self) -> BackendResult<()> {
        let _ = self.event_tx.send(BackendEvent::Compacting(true));
        // TODO: actual compaction
        let _ = self.event_tx.send(BackendEvent::Compacting(false));
        Ok(())
    }

    async fn cycle_edit_mode(&self) -> BackendResult<()> {
        Ok(())
    }

    async fn switch_agent(&self, _id: &str) -> BackendResult<()> {
        Ok(())
    }

    async fn dialog_response(
        &self,
        _id: &str,
        _value: Option<&str>,
        _confirmed: Option<bool>,
        _cancelled: Option<bool>,
    ) -> BackendResult<()> {
        Ok(())
    }

    async fn autocomplete_request(
        &self,
        _text: &str,
        _cursor: usize,
        _force: bool,
    ) -> BackendResult<()> {
        Ok(())
    }

    async fn update_setting(&self, _key: &str, _value: &str) -> BackendResult<()> {
        Ok(())
    }

    async fn connect(&self) -> BackendResult<()> {
        let sess = self.session.lock().await;
        self.push_state(&sess);
        Ok(())
    }

    async fn disconnect(&self) -> BackendResult<()> {
        Ok(())
    }

    async fn exit(&self) -> BackendResult<()> {
        let _ = self.event_tx.send(BackendEvent::Shutdown);
        Ok(())
    }

    fn events(&self) -> mpsc::UnboundedReceiver<BackendEvent> {
        self.event_rx
            .try_lock()
            .ok()
            .and_then(|mut guard| guard.take())
            .expect("events() called more than once")
    }
}
