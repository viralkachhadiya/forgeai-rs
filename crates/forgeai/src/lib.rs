//! High-level forgeai SDK.

use forgeai_core::{
    validate_request, ChatAdapter, ChatRequest, ChatResponse, ForgeError, StreamEvent, StreamResult,
};
use std::sync::Arc;

pub struct Client {
    adapter: Arc<dyn ChatAdapter>,
}

impl Client {
    pub fn new(adapter: Arc<dyn ChatAdapter>) -> Self {
        Self { adapter }
    }

    pub async fn chat(&self, request: ChatRequest) -> Result<ChatResponse, ForgeError> {
        validate_request(&request)?;
        self.adapter.chat(request).await
    }

    pub async fn chat_stream(
        &self,
        request: ChatRequest,
    ) -> Result<StreamResult<StreamEvent>, ForgeError> {
        validate_request(&request)?;
        self.adapter.chat_stream(request).await
    }
}

pub use forgeai_core;
