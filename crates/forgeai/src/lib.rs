//! High-level forgeai SDK.

use forgeai_core::{
    validate_request, ChatAdapter, ChatRequest, ChatResponse, ForgeError, Message, Role,
    StreamEvent, StreamResult, ToolCall, Usage,
};
use forgeai_tools::ToolExecutor;
use serde_json::{json, Value};
use std::collections::HashMap;
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

    pub async fn chat_with_tools(
        &self,
        request: ChatRequest,
        tools: &dyn ToolExecutor,
        options: ToolLoopOptions,
    ) -> Result<ToolLoopResult, ForgeError> {
        run_tool_loop(self, request, tools, options, false).await
    }

    pub async fn chat_with_tools_streaming(
        &self,
        request: ChatRequest,
        tools: &dyn ToolExecutor,
        options: ToolLoopOptions,
    ) -> Result<ToolLoopResult, ForgeError> {
        run_tool_loop(self, request, tools, options, true).await
    }
}

#[derive(Debug, Clone)]
pub struct ToolLoopOptions {
    pub max_iterations: usize,
}

impl Default for ToolLoopOptions {
    fn default() -> Self {
        Self { max_iterations: 8 }
    }
}

#[derive(Debug, Clone)]
pub struct ToolInvocation {
    pub call_id: String,
    pub name: String,
    pub input: Value,
    pub output: Value,
}

#[derive(Debug, Clone)]
pub struct ToolLoopResult {
    pub final_response: ChatResponse,
    pub tool_invocations: Vec<ToolInvocation>,
    pub iterations: usize,
}

async fn run_tool_loop(
    client: &Client,
    mut request: ChatRequest,
    tools: &dyn ToolExecutor,
    options: ToolLoopOptions,
    use_streaming: bool,
) -> Result<ToolLoopResult, ForgeError> {
    validate_request(&request)?;
    if options.max_iterations == 0 {
        return Err(ForgeError::Validation(
            "max_iterations must be greater than 0".to_string(),
        ));
    }

    let mut invocations = Vec::new();

    for iteration in 0..options.max_iterations {
        let response = if use_streaming {
            client.chat_stream_collect(request.clone()).await?
        } else {
            client.adapter.chat(request.clone()).await?
        };

        if response.tool_calls.is_empty() {
            return Ok(ToolLoopResult {
                final_response: response,
                tool_invocations: invocations,
                iterations: iteration + 1,
            });
        }

        request.messages.push(Message {
            role: Role::Assistant,
            content: response.output_text.clone(),
        });

        for call in response.tool_calls {
            let output = tools
                .call(&call.name, call.arguments.clone())
                .map_err(|e| {
                    ForgeError::Provider(format!("tool '{}' execution failed: {e}", call.name))
                })?;

            invocations.push(ToolInvocation {
                call_id: call.id.clone(),
                name: call.name.clone(),
                input: call.arguments.clone(),
                output: output.clone(),
            });

            request.messages.push(Message {
                role: Role::Tool,
                content: json!({
                    "tool_call_id": call.id,
                    "name": call.name,
                    "output": output
                })
                .to_string(),
            });
        }
    }

    Err(ForgeError::Provider(format!(
        "tool loop exceeded max iterations ({})",
        options.max_iterations
    )))
}

impl Client {
    async fn chat_stream_collect(&self, request: ChatRequest) -> Result<ChatResponse, ForgeError> {
        let mut stream = self.chat_stream(request.clone()).await?;
        let mut text = String::new();
        let mut usage: Option<Usage> = None;
        let mut tool_call_deltas: HashMap<String, Value> = HashMap::new();

        use futures_util::StreamExt;
        while let Some(item) = stream.next().await {
            match item? {
                StreamEvent::TextDelta { delta } => text.push_str(&delta),
                StreamEvent::Usage { usage: u } => usage = Some(u),
                StreamEvent::ToolCallDelta { call_id, delta } => {
                    tool_call_deltas.insert(call_id, delta);
                }
                StreamEvent::Done => break,
            }
        }

        let tool_calls = tool_call_deltas
            .into_iter()
            .map(|(call_id, delta)| {
                // Best-effort normalization across provider stream formats.
                let name = delta
                    .get("name")
                    .and_then(Value::as_str)
                    .or_else(|| {
                        delta
                            .get("function")
                            .and_then(|f| f.get("name"))
                            .and_then(Value::as_str)
                    })
                    .unwrap_or("unknown_tool")
                    .to_string();
                let arguments = delta
                    .get("arguments")
                    .cloned()
                    .or_else(|| {
                        delta
                            .get("function")
                            .and_then(|f| f.get("arguments"))
                            .cloned()
                    })
                    .unwrap_or(Value::Null);
                ToolCall {
                    id: call_id,
                    name,
                    arguments,
                }
            })
            .collect();

        Ok(ChatResponse {
            id: "stream-collected".to_string(),
            model: request.model,
            output_text: text,
            tool_calls,
            usage,
        })
    }
}

pub use forgeai_core;
pub use forgeai_tools;

#[cfg(test)]
mod tests {
    use super::*;
    use async_stream::try_stream;
    use async_trait::async_trait;
    use forgeai_core::{AdapterInfo, CapabilityMatrix};
    use serde_json::json;
    use std::collections::VecDeque;
    use std::sync::Mutex;

    struct MockAdapter {
        chat_responses: Mutex<VecDeque<ChatResponse>>,
        stream_responses: Mutex<VecDeque<Vec<StreamEvent>>>,
    }

    impl MockAdapter {
        fn with_chat_responses(items: Vec<ChatResponse>) -> Self {
            Self {
                chat_responses: Mutex::new(VecDeque::from(items)),
                stream_responses: Mutex::new(VecDeque::new()),
            }
        }

        fn with_stream_responses(items: Vec<Vec<StreamEvent>>) -> Self {
            Self {
                chat_responses: Mutex::new(VecDeque::new()),
                stream_responses: Mutex::new(VecDeque::from(items)),
            }
        }
    }

    #[async_trait]
    impl ChatAdapter for MockAdapter {
        fn info(&self) -> AdapterInfo {
            AdapterInfo {
                name: "mock".to_string(),
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
            self.chat_responses
                .lock()
                .map_err(|_| ForgeError::Internal("lock poisoned".to_string()))?
                .pop_front()
                .ok_or_else(|| ForgeError::Internal("no mock chat response remaining".to_string()))
        }

        async fn chat_stream(
            &self,
            _request: ChatRequest,
        ) -> Result<StreamResult<StreamEvent>, ForgeError> {
            let events = self
                .stream_responses
                .lock()
                .map_err(|_| ForgeError::Internal("lock poisoned".to_string()))?
                .pop_front()
                .ok_or_else(|| {
                    ForgeError::Internal("no mock stream response remaining".to_string())
                })?;

            let stream = try_stream! {
                for event in events {
                    yield event;
                }
            };
            Ok(Box::pin(stream))
        }
    }

    struct EchoTools;

    impl ToolExecutor for EchoTools {
        fn call(&self, _name: &str, input: Value) -> Result<Value, forgeai_tools::ToolError> {
            Ok(json!({ "echo": input }))
        }
    }

    fn base_request() -> ChatRequest {
        ChatRequest {
            model: "mock-model".to_string(),
            messages: vec![Message {
                role: Role::User,
                content: "what time is it?".to_string(),
            }],
            temperature: Some(0.1),
            max_tokens: Some(128),
            tools: vec![],
            metadata: json!({}),
        }
    }

    #[tokio::test]
    async fn chat_with_tools_runs_loop_until_final_answer() {
        let adapter = MockAdapter::with_chat_responses(vec![
            ChatResponse {
                id: "1".to_string(),
                model: "mock-model".to_string(),
                output_text: "".to_string(),
                tool_calls: vec![ToolCall {
                    id: "call-1".to_string(),
                    name: "time.now".to_string(),
                    arguments: json!({"timezone":"UTC"}),
                }],
                usage: None,
            },
            ChatResponse {
                id: "2".to_string(),
                model: "mock-model".to_string(),
                output_text: "Current UTC time is 12:00".to_string(),
                tool_calls: vec![],
                usage: None,
            },
        ]);

        let client = Client::new(Arc::new(adapter));
        let result = client
            .chat_with_tools(base_request(), &EchoTools, ToolLoopOptions::default())
            .await
            .unwrap();

        assert_eq!(
            result.final_response.output_text,
            "Current UTC time is 12:00"
        );
        assert_eq!(result.tool_invocations.len(), 1);
        assert_eq!(result.tool_invocations[0].name, "time.now");
        assert_eq!(result.iterations, 2);
    }

    #[tokio::test]
    async fn chat_with_tools_streaming_collects_events_and_executes_tools() {
        let adapter = MockAdapter::with_stream_responses(vec![
            vec![
                StreamEvent::ToolCallDelta {
                    call_id: "call-1".to_string(),
                    delta: json!({"name":"time.now","arguments":{"timezone":"UTC"}}),
                },
                StreamEvent::Done,
            ],
            vec![
                StreamEvent::TextDelta {
                    delta: "Current UTC time is 12:00".to_string(),
                },
                StreamEvent::Done,
            ],
        ]);

        let client = Client::new(Arc::new(adapter));
        let result = client
            .chat_with_tools_streaming(base_request(), &EchoTools, ToolLoopOptions::default())
            .await
            .unwrap();

        assert_eq!(
            result.final_response.output_text,
            "Current UTC time is 12:00"
        );
        assert_eq!(result.tool_invocations.len(), 1);
        assert_eq!(result.iterations, 2);
    }

    #[tokio::test]
    async fn chat_with_tools_honors_max_iterations() {
        let adapter = MockAdapter::with_chat_responses(vec![ChatResponse {
            id: "1".to_string(),
            model: "mock-model".to_string(),
            output_text: "".to_string(),
            tool_calls: vec![ToolCall {
                id: "call-1".to_string(),
                name: "loop.forever".to_string(),
                arguments: json!({}),
            }],
            usage: None,
        }]);

        let client = Client::new(Arc::new(adapter));
        let err = client
            .chat_with_tools(
                base_request(),
                &EchoTools,
                ToolLoopOptions { max_iterations: 1 },
            )
            .await
            .unwrap_err();

        assert!(matches!(err, ForgeError::Provider(_)));
    }
}
