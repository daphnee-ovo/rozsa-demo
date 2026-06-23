use std::collections::HashMap;

use rozsa_core::messages::AgentMessage;
use tracing::warn;

#[derive(Debug, Clone)]
pub struct SessionStartHook {
    pub session_id: String,
    pub cwd: String,
}

#[derive(Debug, Clone)]
pub struct AfterProviderResponseHook {
    pub status: u16,
    pub headers: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct ToolCallHook {
    pub tool_name: String,
    pub tool_call_id: String,
    pub args: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct ToolResultHook {
    pub tool_name: String,
    pub tool_call_id: String,
    pub result: serde_json::Value,
    pub is_error: bool,
}

#[async_trait::async_trait]
pub trait Extension: Send + Sync {
    fn name(&self) -> &str;

    async fn on_session_start(&self, _event: &SessionStartHook) -> anyhow::Result<()> {
        Ok(())
    }

    async fn on_before_provider_request(
        &self,
        payload: serde_json::Value,
    ) -> anyhow::Result<serde_json::Value> {
        Ok(payload)
    }

    async fn on_after_provider_response(
        &self,
        _event: &AfterProviderResponseHook,
    ) -> anyhow::Result<()> {
        Ok(())
    }

    async fn on_tool_call(&self, _event: &ToolCallHook) -> anyhow::Result<()> {
        Ok(())
    }

    async fn on_tool_result(&self, _event: &ToolResultHook) -> anyhow::Result<()> {
        Ok(())
    }

    async fn on_context(
        &self,
        messages: Vec<AgentMessage>,
    ) -> anyhow::Result<Vec<AgentMessage>> {
        Ok(messages)
    }
}

pub struct ExtensionRunner {
    extensions: Vec<Box<dyn Extension>>,
}

impl ExtensionRunner {
    pub fn new() -> Self {
        Self {
            extensions: Vec::new(),
        }
    }

    pub fn register(&mut self, extension: Box<dyn Extension>) {
        self.extensions.push(extension);
    }

    pub fn has_extensions(&self) -> bool {
        !self.extensions.is_empty()
    }

    pub async fn emit_session_start(&self, event: &SessionStartHook) {
        for (i, ext) in self.extensions.iter().enumerate() {
            if let Err(e) = ext.on_session_start(event).await {
                warn!(extension_index = i, error = %e, "Extension session_start hook failed");
            }
        }
    }

    pub async fn emit_before_provider_request(
        &self,
        mut payload: serde_json::Value,
    ) -> serde_json::Value {
        for (i, ext) in self.extensions.iter().enumerate() {
            match ext.on_before_provider_request(payload.clone()).await {
                Ok(transformed) => payload = transformed,
                Err(e) => {
                    warn!(extension_index = i, error = %e, "Extension before_provider_request failed, stopping chain");
                    break;
                }
            }
        }
        payload
    }

    pub async fn emit_after_provider_response(&self, event: &AfterProviderResponseHook) {
        for (i, ext) in self.extensions.iter().enumerate() {
            if let Err(e) = ext.on_after_provider_response(event).await {
                warn!(extension_index = i, error = %e, "Extension after_provider_response hook failed");
            }
        }
    }

    pub async fn emit_tool_call(&self, event: &ToolCallHook) {
        for (i, ext) in self.extensions.iter().enumerate() {
            if let Err(e) = ext.on_tool_call(event).await {
                warn!(extension_index = i, error = %e, "Extension tool_call hook failed");
            }
        }
    }

    pub async fn emit_tool_result(&self, event: &ToolResultHook) {
        for (i, ext) in self.extensions.iter().enumerate() {
            if let Err(e) = ext.on_tool_result(event).await {
                warn!(extension_index = i, error = %e, "Extension tool_result hook failed");
            }
        }
    }

    pub async fn emit_context(&self, mut messages: Vec<AgentMessage>) -> Vec<AgentMessage> {
        for (i, ext) in self.extensions.iter().enumerate() {
            match ext.on_context(messages.clone()).await {
                Ok(transformed) => messages = transformed,
                Err(e) => {
                    warn!(extension_index = i, error = %e, "Extension context hook failed, stopping chain");
                    break;
                }
            }
        }
        messages
    }
}

impl Default for ExtensionRunner {
    fn default() -> Self {
        Self::new()
    }
}
