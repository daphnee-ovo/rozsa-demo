// FrameworkTree
// ask_user_question.rs
// ├── struct AskUserQuestionParams
// ├── struct AskUserQuestion
// ├── struct AskUserQuestionOption
// ├── enum AskUserQuestionAnswer
// ├── enum AskUserQuestionResponse
// ├── struct AskUserQuestionRequest
// ├── struct AskUserQuestionTool
// ├── impl AskUserQuestionTool
// ├── new()
// ├── impl AskUserQuestionTool
// ├── name()
// ├── description()
// ├── label()
// ├── parameters_schema()
// ├── execution_mode()
// ├── execute()
// ├── create_ask_user_question_tool()
// ├── validate_params()
// └── validate_answers()

use std::collections::{BTreeMap, BTreeSet};

use rozsa_core::tool::{Tool, ToolError, ToolExecutionMode, ToolResult};
use rozsa_model::types::ContentBlock;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

pub const ASK_USER_QUESTION_TOOL_NAME: &str = "askUserQuestion";
const MAX_QUESTIONS: usize = 4;
const MAX_OPTIONS: usize = 8;

/// The agent-facing payload for askUserQuestion.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct AskUserQuestionParams {
    pub questions: Vec<AskUserQuestion>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct AskUserQuestion {
    pub question: String,
    pub header: String,
    pub options: Vec<AskUserQuestionOption>,
    #[serde(rename = "multiSelect", alias = "multi_select", default)]
    pub multi_select: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct AskUserQuestionOption {
    pub label: String,
    pub description: String,
}

/// The response sent from the interactive frontend back to the waiting tool.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum AskUserQuestionAnswer {
    Single(String),
    Multiple(Vec<String>),
}

/// Answered preserves the agent-facing result shape. Cancelled is used for
/// abort, session deletion, window close, and explicit UI cancellation.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub enum AskUserQuestionResponse {
    Answered {
        answers: BTreeMap<String, AskUserQuestionAnswer>,
    },
    Cancelled,
}

/// A request crossing from the app runtime to an interactive frontend. The
/// response sender is intentionally not serializable; only the event payload
/// derived from this request is sent to the WebView.
pub struct AskUserQuestionRequest {
    pub session_id: String,
    pub request_id: String,
    pub questions: Vec<AskUserQuestion>,
    pub response_tx: oneshot::Sender<AskUserQuestionResponse>,
}

pub type AskUserQuestionRequestSender = mpsc::UnboundedSender<AskUserQuestionRequest>;

pub struct AskUserQuestionTool {
    session_id: String,
    request_tx: AskUserQuestionRequestSender,
}

impl AskUserQuestionTool {
    pub fn new(session_id: String, request_tx: AskUserQuestionRequestSender) -> Self {
        Self {
            session_id,
            request_tx,
        }
    }
}

#[async_trait::async_trait]
impl Tool for AskUserQuestionTool {
    fn name(&self) -> &str {
        ASK_USER_QUESTION_TOOL_NAME
    }

    fn description(&self) -> &str {
        "Ask the user one or more structured questions with single-select or multi-select options. The GUI always includes an Other option for custom input, and that option cannot be disabled. Use this when a decision or missing preference requires explicit user input."
    }

    fn label(&self) -> &str {
        "Ask User Question"
    }

    fn parameters_schema(&self) -> &Value {
        static SCHEMA: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
        SCHEMA.get_or_init(|| {
            json!({
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "questions": {
                        "type": "array",
                        "minItems": 1,
                        "maxItems": MAX_QUESTIONS,
                        "description": "Questions to show the user. Each question is answered before the agent continues.",
                        "items": {
                            "type": "object",
                            "additionalProperties": false,
                            "properties": {
                                "question": {
                                    "type": "string",
                                    "description": "The question shown to the user"
                                },
                                "header": {
                                    "type": "string",
                                    "description": "Short unique key used in the answers object"
                                },
                                "options": {
                                    "type": "array",
                                    "minItems": 1,
                                    "maxItems": MAX_OPTIONS,
                                    "items": {
                                        "type": "object",
                                        "additionalProperties": false,
                                        "properties": {
                                            "label": {
                                                "type": "string",
                                                "description": "The selectable option label; this is returned in the answer"
                                            },
                                            "description": {
                                                "type": "string",
                                                "description": "Additional context shown below the option label"
                                            }
                                        },
                                        "required": ["label", "description"]
                                    }
                                },
                                "multiSelect": {
                                    "type": "boolean",
                                    "default": false,
                                    "description": "Allow the user to select more than one option"
                                },
                            },
                            "required": ["question", "header", "options"]
                        }
                    }
                },
                "required": ["questions"]
            })
        })
    }

    fn execution_mode(&self) -> Option<ToolExecutionMode> {
        Some(ToolExecutionMode::Sequential)
    }

    async fn execute(
        &self,
        tool_call_id: &str,
        params: Value,
        signal: Option<CancellationToken>,
        _on_update: Option<&(dyn Fn(ToolResult) + Send + Sync)>,
    ) -> Result<ToolResult, ToolError> {
        if signal.as_ref().is_some_and(CancellationToken::is_cancelled) {
            return Err(ToolError::Cancelled);
        }

        let params: AskUserQuestionParams = serde_json::from_value(params).map_err(|error| {
            ToolError::Execution(format!("Invalid askUserQuestion parameters: {error}"))
        })?;
        validate_params(&params)?;

        let (response_tx, response_rx) = oneshot::channel();
        self.request_tx
            .send(AskUserQuestionRequest {
                session_id: self.session_id.clone(),
                request_id: tool_call_id.to_string(),
                questions: params.questions.clone(),
                response_tx,
            })
            .map_err(|_| {
                ToolError::Execution(
                    "askUserQuestion is unavailable because no interactive frontend is connected"
                        .to_string(),
                )
            })?;

        let response = match signal {
            Some(signal) => {
                tokio::select! {
                    biased;
                    _ = signal.cancelled() => return Err(ToolError::Cancelled),
                    response = response_rx => response,
                }
            }
            None => response_rx.await,
        }
        .map_err(|_| ToolError::Execution("askUserQuestion response channel closed".to_string()))?;

        let AskUserQuestionResponse::Answered { answers } = response else {
            return Err(ToolError::Execution(
                "User cancelled askUserQuestion".to_string(),
            ));
        };
        validate_answers(&params.questions, &answers)?;

        let details = json!({ "answers": answers });
        let content = serde_json::to_string(&details).map_err(|error| {
            ToolError::Execution(format!("Failed to encode askUserQuestion result: {error}"))
        })?;
        Ok(ToolResult {
            content: vec![ContentBlock::Text {
                text: content,
                signature: None,
            }],
            details,
            terminate: false,
        })
    }
}

pub fn create_ask_user_question_tool(
    session_id: String,
    request_tx: AskUserQuestionRequestSender,
) -> Box<dyn Tool> {
    Box::new(AskUserQuestionTool::new(session_id, request_tx))
}

pub fn validate_params(params: &AskUserQuestionParams) -> Result<(), ToolError> {
    if params.questions.is_empty() || params.questions.len() > MAX_QUESTIONS {
        return Err(ToolError::Execution(format!(
            "askUserQuestion requires 1 to {MAX_QUESTIONS} questions"
        )));
    }

    let mut headers = BTreeSet::new();
    for (question_index, question) in params.questions.iter().enumerate() {
        if question.question.trim().is_empty() {
            return Err(ToolError::Execution(format!(
                "askUserQuestion question {question_index} must not be empty"
            )));
        }
        if question.header.trim().is_empty() {
            return Err(ToolError::Execution(format!(
                "askUserQuestion question {question_index} header must not be empty"
            )));
        }
        if !headers.insert(question.header.clone()) {
            return Err(ToolError::Execution(format!(
                "askUserQuestion headers must be unique: '{}' is repeated",
                question.header
            )));
        }
        if question.options.is_empty() || question.options.len() > MAX_OPTIONS {
            return Err(ToolError::Execution(format!(
                "askUserQuestion question '{}' requires 1 to {MAX_OPTIONS} options",
                question.header
            )));
        }

        let mut labels = BTreeSet::new();
        for option in &question.options {
            if option.label.trim().is_empty() {
                return Err(ToolError::Execution(format!(
                    "askUserQuestion option labels must not be empty for '{}'",
                    question.header
                )));
            }
            if option.description.trim().is_empty() {
                return Err(ToolError::Execution(format!(
                    "askUserQuestion option '{}' must include a description",
                    option.label
                )));
            }
            if !labels.insert(option.label.clone()) {
                return Err(ToolError::Execution(format!(
                    "askUserQuestion option labels must be unique for '{}'",
                    question.header
                )));
            }
        }
    }
    Ok(())
}

pub fn validate_answers(
    questions: &[AskUserQuestion],
    answers: &BTreeMap<String, AskUserQuestionAnswer>,
) -> Result<(), ToolError> {
    if answers.len() != questions.len() {
        return Err(ToolError::Execution(
            "askUserQuestion result must answer every question exactly once".to_string(),
        ));
    }

    for question in questions {
        let Some(answer) = answers.get(&question.header) else {
            return Err(ToolError::Execution(format!(
                "askUserQuestion result is missing answer for '{}'",
                question.header
            )));
        };
        let selected: Vec<&str> = match (question.multi_select, answer) {
            (false, AskUserQuestionAnswer::Single(label)) => vec![label.as_str()],
            (true, AskUserQuestionAnswer::Multiple(labels)) => {
                labels.iter().map(String::as_str).collect()
            }
            (false, AskUserQuestionAnswer::Multiple(_)) => {
                return Err(ToolError::Execution(format!(
                    "askUserQuestion answer for '{}' must be single-select",
                    question.header
                )));
            }
            (true, AskUserQuestionAnswer::Single(_)) => {
                return Err(ToolError::Execution(format!(
                    "askUserQuestion answer for '{}' must be multi-select",
                    question.header
                )));
            }
        };
        if selected.is_empty() {
            return Err(ToolError::Execution(format!(
                "askUserQuestion answer for '{}' must select at least one option",
                question.header
            )));
        }
        let mut seen = BTreeSet::new();
        for label in selected {
            if !seen.insert(label) {
                return Err(ToolError::Execution(format!(
                    "askUserQuestion answer for '{}' contains duplicate option '{}'",
                    question.header, label
                )));
            }
        }
    }
    Ok(())
}
