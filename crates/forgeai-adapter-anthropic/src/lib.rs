use async_trait::async_trait;
use forgeai_core::{
    AdapterInfo, CapabilityMatrix, ChatAdapter, ChatRequest, ChatResponse, ForgeError, StreamEvent,
    StreamResult,
};
use url::Url;
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct AnthropicAdapter;

#[async_trait]
impl ChatAdapter for AnthropicAdapter {
    fn info(&self) -> AdapterInfo {
        AdapterInfo {
            name: "anthropic".to_string(),
            base_url: Url::parse("https://api.anthropic.com").ok(),
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
        Ok(ChatResponse {
            id: Uuid::new_v4().to_string(),
            model: request.model,
            output_text: "stubbed anthropic response".to_string(),
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
