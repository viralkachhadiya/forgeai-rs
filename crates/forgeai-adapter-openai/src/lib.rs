use async_trait::async_trait;
use forgeai_core::{
    AdapterInfo, CapabilityMatrix, ChatAdapter, ChatRequest, ChatResponse, ForgeError, StreamEvent,
    StreamResult,
};
use std::env;
use url::Url;
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct OpenAiAdapter {
    pub api_key: String,
    pub base_url: Url,
}

impl OpenAiAdapter {
    pub fn from_env() -> Result<Self, ForgeError> {
        let api_key = env::var("OPENAI_API_KEY").map_err(|_| ForgeError::Authentication)?;
        let base_url = Url::parse("https://api.openai.com")
            .map_err(|e| ForgeError::Internal(e.to_string()))?;
        Ok(Self { api_key, base_url })
    }
}

#[async_trait]
impl ChatAdapter for OpenAiAdapter {
    fn info(&self) -> AdapterInfo {
        AdapterInfo {
            name: "openai".to_string(),
            base_url: Some(self.base_url.clone()),
            capabilities: CapabilityMatrix {
                streaming: true,
                tools: true,
                structured_output: true,
                multimodal_input: true,
                citations: false,
            },
        }
    }

    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse, ForgeError> {
        let prompt = request
            .messages
            .last()
            .map(|m| m.content.clone())
            .unwrap_or_default();
        Ok(ChatResponse {
            id: Uuid::new_v4().to_string(),
            model: request.model,
            output_text: format!("stubbed openai response: {prompt}"),
            tool_calls: vec![],
            usage: None,
        })
    }

    async fn chat_stream(
        &self,
        _request: ChatRequest,
    ) -> Result<StreamResult<StreamEvent>, ForgeError> {
        Err(ForgeError::Provider(
            "streaming is not implemented in the scaffold".to_string(),
        ))
    }
}
