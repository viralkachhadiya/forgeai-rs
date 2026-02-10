//! Core domain types and adapter traits for forgeai-rs.

use async_trait::async_trait;
use futures_core::Stream;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::pin::Pin;
use url::Url;

pub type BoxError = Box<dyn std::error::Error + Send + Sync + 'static>;
pub type StreamResult<T> = Pin<Box<dyn Stream<Item = Result<T, ForgeError>> + Send>>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<Message>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    pub tools: Vec<ToolDefinition>,
    pub metadata: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: Option<String>,
    pub input_schema: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    pub id: String,
    pub model: String,
    pub output_text: String,
    pub tool_calls: Vec<ToolCall>,
    pub usage: Option<Usage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Usage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub total_tokens: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StreamEvent {
    TextDelta { delta: String },
    ToolCallDelta { call_id: String, delta: Value },
    Usage { usage: Usage },
    Done,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityMatrix {
    pub streaming: bool,
    pub tools: bool,
    pub structured_output: bool,
    pub multimodal_input: bool,
    pub citations: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterInfo {
    pub name: String,
    pub base_url: Option<Url>,
    pub capabilities: CapabilityMatrix,
}

#[derive(Debug, thiserror::Error)]
pub enum ForgeError {
    #[error("validation error: {0}")]
    Validation(String),
    #[error("authentication error")]
    Authentication,
    #[error("rate limited")]
    RateLimited,
    #[error("provider error: {0}")]
    Provider(String),
    #[error("transport error: {0}")]
    Transport(String),
    #[error("internal error: {0}")]
    Internal(String),
}

#[async_trait]
pub trait ChatAdapter: Send + Sync {
    fn info(&self) -> AdapterInfo;

    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse, ForgeError>;

    async fn chat_stream(
        &self,
        request: ChatRequest,
    ) -> Result<StreamResult<StreamEvent>, ForgeError>;
}

pub fn validate_request(request: &ChatRequest) -> Result<(), ForgeError> {
    if request.model.trim().is_empty() {
        return Err(ForgeError::Validation("model cannot be empty".to_string()));
    }
    if request.messages.is_empty() {
        return Err(ForgeError::Validation(
            "messages cannot be empty".to_string(),
        ));
    }
    Ok(())
}
