use async_trait::async_trait;
use forgeai_core::{
    AdapterInfo, ChatAdapter, ChatRequest, ChatResponse, ForgeError, StreamEvent, StreamResult,
};
use std::sync::Arc;

pub fn pick_first_healthy(adapters: &[AdapterInfo]) -> Option<&AdapterInfo> {
    adapters.first()
}

#[derive(Debug, Clone, Copy)]
pub struct FailoverPolicy {
    pub max_adapters_to_try: usize,
}

impl Default for FailoverPolicy {
    fn default() -> Self {
        Self {
            max_adapters_to_try: usize::MAX,
        }
    }
}

pub struct FailoverRouter {
    adapters: Vec<Arc<dyn ChatAdapter>>,
    policy: FailoverPolicy,
}

impl FailoverRouter {
    pub fn new(adapters: Vec<Arc<dyn ChatAdapter>>) -> Result<Self, ForgeError> {
        Self::with_policy(adapters, FailoverPolicy::default())
    }

    pub fn with_policy(
        adapters: Vec<Arc<dyn ChatAdapter>>,
        policy: FailoverPolicy,
    ) -> Result<Self, ForgeError> {
        if adapters.is_empty() {
            return Err(ForgeError::Validation(
                "failover router requires at least one adapter".to_string(),
            ));
        }
        Ok(Self { adapters, policy })
    }

    fn adapters_to_try(&self) -> impl Iterator<Item = &Arc<dyn ChatAdapter>> {
        self.adapters.iter().take(self.policy.max_adapters_to_try)
    }
}

#[async_trait]
impl ChatAdapter for FailoverRouter {
    fn info(&self) -> AdapterInfo {
        let first = self.adapters[0].info();
        AdapterInfo {
            name: "failover-router".to_string(),
            base_url: first.base_url,
            capabilities: first.capabilities,
        }
    }

    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse, ForgeError> {
        let mut last_error: Option<ForgeError> = None;
        for adapter in self.adapters_to_try() {
            match adapter.chat(request.clone()).await {
                Ok(response) => return Ok(response),
                Err(error) if should_failover(&error) => {
                    last_error = Some(error);
                }
                Err(error) => return Err(error),
            }
        }
        Err(last_error.unwrap_or_else(|| {
            ForgeError::Internal("failover router exhausted adapters without error".to_string())
        }))
    }

    async fn chat_stream(
        &self,
        request: ChatRequest,
    ) -> Result<StreamResult<StreamEvent>, ForgeError> {
        let mut last_error: Option<ForgeError> = None;
        for adapter in self.adapters_to_try() {
            match adapter.chat_stream(request.clone()).await {
                Ok(stream) => return Ok(stream),
                Err(error) if should_failover(&error) => {
                    last_error = Some(error);
                }
                Err(error) => return Err(error),
            }
        }
        Err(last_error.unwrap_or_else(|| {
            ForgeError::Internal("failover router exhausted adapters without error".to_string())
        }))
    }
}

fn should_failover(error: &ForgeError) -> bool {
    matches!(
        error,
        ForgeError::RateLimited | ForgeError::Transport(_) | ForgeError::Provider(_)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use forgeai_core::{CapabilityMatrix, Message, Role};

    struct MockAdapter {
        name: String,
        result: Result<ChatResponse, ForgeError>,
    }

    #[async_trait]
    impl ChatAdapter for MockAdapter {
        fn info(&self) -> AdapterInfo {
            AdapterInfo {
                name: self.name.clone(),
                base_url: None,
                capabilities: CapabilityMatrix {
                    streaming: true,
                    tools: true,
                    structured_output: true,
                    multimodal_input: false,
                    citations: false,
                },
            }
        }

        async fn chat(&self, _request: ChatRequest) -> Result<ChatResponse, ForgeError> {
            match &self.result {
                Ok(response) => Ok(response.clone()),
                Err(ForgeError::Validation(message)) => {
                    Err(ForgeError::Validation(message.clone()))
                }
                Err(ForgeError::Authentication) => Err(ForgeError::Authentication),
                Err(ForgeError::RateLimited) => Err(ForgeError::RateLimited),
                Err(ForgeError::Provider(message)) => Err(ForgeError::Provider(message.clone())),
                Err(ForgeError::Transport(message)) => Err(ForgeError::Transport(message.clone())),
                Err(ForgeError::Internal(message)) => Err(ForgeError::Internal(message.clone())),
            }
        }

        async fn chat_stream(
            &self,
            _request: ChatRequest,
        ) -> Result<StreamResult<StreamEvent>, ForgeError> {
            Err(ForgeError::Provider(
                "stream tests are out of scope for this unit test".to_string(),
            ))
        }
    }

    fn request() -> ChatRequest {
        ChatRequest {
            model: "mock".to_string(),
            messages: vec![Message {
                role: Role::User,
                content: "hello".to_string(),
            }],
            temperature: None,
            max_tokens: None,
            tools: vec![],
            metadata: serde_json::json!({}),
        }
    }

    #[tokio::test]
    async fn router_returns_first_successful_adapter() {
        let router = FailoverRouter::new(vec![
            Arc::new(MockAdapter {
                name: "a".to_string(),
                result: Err(ForgeError::Transport("timeout".to_string())),
            }),
            Arc::new(MockAdapter {
                name: "b".to_string(),
                result: Ok(ChatResponse {
                    id: "2".to_string(),
                    model: "mock".to_string(),
                    output_text: "ok".to_string(),
                    tool_calls: vec![],
                    usage: None,
                }),
            }),
        ])
        .unwrap();

        let response = router.chat(request()).await.unwrap();
        assert_eq!(response.output_text, "ok");
    }

    #[tokio::test]
    async fn router_stops_on_non_retryable_error() {
        let router = FailoverRouter::new(vec![
            Arc::new(MockAdapter {
                name: "a".to_string(),
                result: Err(ForgeError::Authentication),
            }),
            Arc::new(MockAdapter {
                name: "b".to_string(),
                result: Ok(ChatResponse {
                    id: "2".to_string(),
                    model: "mock".to_string(),
                    output_text: "should not be used".to_string(),
                    tool_calls: vec![],
                    usage: None,
                }),
            }),
        ])
        .unwrap();

        let err = router.chat(request()).await.unwrap_err();
        assert!(matches!(err, ForgeError::Authentication));
    }
}
