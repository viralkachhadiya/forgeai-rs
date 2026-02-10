use async_stream::try_stream;
use async_trait::async_trait;
use forgeai_core::{
    AdapterInfo, CapabilityMatrix, ChatAdapter, ChatRequest, ChatResponse, ForgeError, Role,
    StreamEvent, StreamResult, ToolCall, Usage,
};
use futures_util::StreamExt;
use reqwest::{Client as HttpClient, StatusCode};
use serde_json::{json, Map, Value};
use std::env;
use url::Url;

#[derive(Clone, Debug)]
pub struct AnthropicAdapter {
    pub api_key: String,
    pub base_url: Url,
    pub api_version: String,
    client: HttpClient,
}

impl AnthropicAdapter {
    pub fn new(api_key: impl Into<String>) -> Result<Self, ForgeError> {
        let base_url = Url::parse("https://api.anthropic.com")
            .map_err(|e| ForgeError::Internal(e.to_string()))?;
        Self::with_base_url(api_key, base_url)
    }

    pub fn with_base_url(api_key: impl Into<String>, base_url: Url) -> Result<Self, ForgeError> {
        let client = HttpClient::builder()
            .build()
            .map_err(|e| ForgeError::Internal(format!("failed to build http client: {e}")))?;
        Ok(Self {
            api_key: api_key.into(),
            base_url,
            api_version: "2023-06-01".to_string(),
            client,
        })
    }

    pub fn from_env() -> Result<Self, ForgeError> {
        let api_key = env::var("ANTHROPIC_API_KEY").map_err(|_| ForgeError::Authentication)?;
        match env::var("ANTHROPIC_BASE_URL") {
            Ok(raw) => {
                let base_url = Url::parse(&raw).map_err(|e| {
                    ForgeError::Validation(format!("invalid ANTHROPIC_BASE_URL: {e}"))
                })?;
                Self::with_base_url(api_key, base_url)
            }
            Err(_) => Self::new(api_key),
        }
    }

    fn messages_url(&self) -> Result<Url, ForgeError> {
        self.base_url
            .join("v1/messages")
            .map_err(|e| ForgeError::Internal(format!("failed to construct endpoint url: {e}")))
    }
}

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
        let response = self
            .client
            .post(self.messages_url()?)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", &self.api_version)
            .json(&build_messages_body(request, false))
            .send()
            .await
            .map_err(|e| ForgeError::Transport(format!("request failed: {e}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response
                .text()
                .await
                .unwrap_or_else(|_| "failed to read error body".to_string());
            return Err(parse_http_error(status, text));
        }

        let payload = response
            .json::<Value>()
            .await
            .map_err(|e| ForgeError::Provider(format!("invalid json response: {e}")))?;
        parse_chat_response(payload)
    }

    async fn chat_stream(
        &self,
        request: ChatRequest,
    ) -> Result<StreamResult<StreamEvent>, ForgeError> {
        let response = self
            .client
            .post(self.messages_url()?)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", &self.api_version)
            .json(&build_messages_body(request, true))
            .send()
            .await
            .map_err(|e| ForgeError::Transport(format!("stream request failed: {e}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response
                .text()
                .await
                .unwrap_or_else(|_| "failed to read error body".to_string());
            return Err(parse_http_error(status, text));
        }

        let mut bytes = response.bytes_stream();
        let stream = try_stream! {
            let mut buffer = String::new();
            let mut saw_done = false;
            let mut event_name: Option<String> = None;
            let mut data_lines: Vec<String> = Vec::new();

            while let Some(chunk) = bytes.next().await {
                let chunk = chunk.map_err(|e| ForgeError::Transport(format!("stream chunk error: {e}")))?;
                let text = std::str::from_utf8(&chunk)
                    .map_err(|e| ForgeError::Transport(format!("invalid utf8 stream chunk: {e}")))?;
                buffer.push_str(text);

                while let Some(line_end) = buffer.find('\n') {
                    let mut line = buffer[..line_end].to_string();
                    buffer.drain(..=line_end);
                    if line.ends_with('\r') {
                        line.pop();
                    }
                    if line.is_empty() {
                        if !data_lines.is_empty() {
                            let payload = data_lines.join("\n");
                            let events = parse_stream_payload(&payload, event_name.as_deref())?;
                            for event in events {
                                if matches!(event, StreamEvent::Done) {
                                    saw_done = true;
                                }
                                yield event;
                            }
                            data_lines.clear();
                            event_name = None;
                        }
                        continue;
                    }
                    if let Some(name) = line.strip_prefix("event:") {
                        event_name = Some(name.trim().to_string());
                        continue;
                    }
                    if let Some(data) = line.strip_prefix("data:") {
                        data_lines.push(data.trim().to_string());
                    }
                }
            }

            if !data_lines.is_empty() {
                let payload = data_lines.join("\n");
                let events = parse_stream_payload(&payload, event_name.as_deref())?;
                for event in events {
                    if matches!(event, StreamEvent::Done) {
                        saw_done = true;
                    }
                    yield event;
                }
            }

            if !saw_done {
                yield StreamEvent::Done;
            }
        };

        Ok(Box::pin(stream))
    }
}

fn build_messages_body(request: ChatRequest, stream: bool) -> Value {
    let mut body = Map::new();
    body.insert("model".to_string(), Value::String(request.model));
    body.insert(
        "max_tokens".to_string(),
        Value::Number((request.max_tokens.unwrap_or(1024)).into()),
    );

    if let Some(temperature) = request.temperature {
        body.insert("temperature".to_string(), json!(temperature));
    }

    let mut system_chunks = Vec::new();
    let mut messages = Vec::new();
    for message in request.messages {
        if matches!(message.role, Role::System) {
            system_chunks.push(message.content);
            continue;
        }
        let role = match message.role {
            Role::Assistant => "assistant",
            _ => "user",
        };
        messages.push(json!({
            "role": role,
            "content": [{ "type": "text", "text": message.content }]
        }));
    }
    body.insert("messages".to_string(), Value::Array(messages));

    if !system_chunks.is_empty() {
        body.insert(
            "system".to_string(),
            Value::String(system_chunks.join("\n\n")),
        );
    }

    if !request.tools.is_empty() {
        body.insert(
            "tools".to_string(),
            Value::Array(
                request
                    .tools
                    .into_iter()
                    .map(|tool| {
                        json!({
                            "name": tool.name,
                            "description": tool.description,
                            "input_schema": tool.input_schema
                        })
                    })
                    .collect(),
            ),
        );
    }

    if stream {
        body.insert("stream".to_string(), Value::Bool(true));
    }

    Value::Object(body)
}

fn parse_http_error(status: StatusCode, body: String) -> ForgeError {
    let message = extract_provider_error(body);
    match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => ForgeError::Authentication,
        StatusCode::TOO_MANY_REQUESTS => ForgeError::RateLimited,
        _ => ForgeError::Provider(message),
    }
}

fn extract_provider_error(body: String) -> String {
    serde_json::from_str::<Value>(&body)
        .ok()
        .and_then(|v| {
            v.get("error")
                .and_then(|e| e.get("message"))
                .and_then(Value::as_str)
                .map(ToString::to_string)
        })
        .unwrap_or(body)
}

fn parse_chat_response(payload: Value) -> Result<ChatResponse, ForgeError> {
    let id = payload
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let model = payload
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();

    let content = payload
        .get("content")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let output_text = extract_text_blocks(&content);
    let tool_calls = extract_tool_calls_from_blocks(&content);
    let usage = extract_usage(payload.get("usage"));

    Ok(ChatResponse {
        id,
        model,
        output_text,
        tool_calls,
        usage,
    })
}

fn extract_text_blocks(content: &[Value]) -> String {
    content
        .iter()
        .filter(|block| block.get("type").and_then(Value::as_str) == Some("text"))
        .filter_map(|block| block.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("")
}

fn extract_tool_calls_from_blocks(content: &[Value]) -> Vec<ToolCall> {
    content
        .iter()
        .filter(|block| block.get("type").and_then(Value::as_str) == Some("tool_use"))
        .map(|block| ToolCall {
            id: block
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            name: block
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            arguments: block.get("input").cloned().unwrap_or(Value::Null),
        })
        .collect()
}

fn extract_usage(raw: Option<&Value>) -> Option<Usage> {
    let usage = raw?;
    let input_tokens = usage
        .get("input_tokens")
        .and_then(Value::as_u64)
        .unwrap_or(0) as u32;
    let output_tokens = usage
        .get("output_tokens")
        .and_then(Value::as_u64)
        .unwrap_or(0) as u32;
    Some(Usage {
        input_tokens,
        output_tokens,
        total_tokens: input_tokens.saturating_add(output_tokens),
    })
}

fn parse_stream_payload(
    payload: &str,
    event: Option<&str>,
) -> Result<Vec<StreamEvent>, ForgeError> {
    let value = serde_json::from_str::<Value>(payload)
        .map_err(|e| ForgeError::Provider(format!("invalid stream payload: {e}")))?;
    let event_type = event
        .map(ToString::to_string)
        .or_else(|| {
            value
                .get("type")
                .and_then(Value::as_str)
                .map(ToString::to_string)
        })
        .unwrap_or_default();

    let mut events = Vec::new();

    if let Some(usage) = value
        .get("usage")
        .and_then(|v| extract_usage(Some(v)))
        .or_else(|| {
            value
                .get("message")
                .and_then(|m| m.get("usage"))
                .and_then(|v| extract_usage(Some(v)))
        })
    {
        events.push(StreamEvent::Usage { usage });
    }

    if event_type == "content_block_delta" {
        if let Some(delta_text) = value
            .get("delta")
            .and_then(|d| d.get("text"))
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
        {
            events.push(StreamEvent::TextDelta {
                delta: delta_text.to_string(),
            });
        }
    }

    if event_type == "content_block_start" {
        if let Some(block) = value.get("content_block") {
            if block.get("type").and_then(Value::as_str) == Some("tool_use") {
                let call_id = block
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                events.push(StreamEvent::ToolCallDelta {
                    call_id,
                    delta: block.clone(),
                });
            }
        }
    }

    if event_type == "message_stop" {
        events.push(StreamEvent::Done);
    }

    Ok(events)
}

#[cfg(test)]
mod tests {
    use super::*;
    use forgeai_core::{ChatRequest, Message, Role};
    use futures_util::StreamExt;
    use wiremock::matchers::{body_partial_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn sample_request() -> ChatRequest {
        ChatRequest {
            model: "claude-3-5-sonnet-latest".to_string(),
            messages: vec![Message {
                role: Role::User,
                content: "Say hello".to_string(),
            }],
            temperature: Some(0.2),
            max_tokens: Some(128),
            tools: vec![],
            metadata: json!({}),
        }
    }

    #[tokio::test]
    async fn chat_contract_parses_response_and_usage() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .and(header("x-api-key", "test-key"))
            .and(header("anthropic-version", "2023-06-01"))
            .and(body_partial_json(
                json!({"model": "claude-3-5-sonnet-latest"}),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "msg_123",
                "model": "claude-3-5-sonnet-latest",
                "content": [{ "type": "text", "text": "Hello from Anthropic" }],
                "usage": {"input_tokens": 12, "output_tokens": 5}
            })))
            .mount(&server)
            .await;

        let adapter =
            AnthropicAdapter::with_base_url("test-key", Url::parse(&server.uri()).unwrap())
                .unwrap();
        let response = adapter.chat(sample_request()).await.unwrap();

        assert_eq!(response.id, "msg_123");
        assert_eq!(response.model, "claude-3-5-sonnet-latest");
        assert_eq!(response.output_text, "Hello from Anthropic");
        assert_eq!(response.usage.unwrap().total_tokens, 17);
    }

    #[tokio::test]
    async fn chat_stream_contract_parses_sse_events() {
        let server = MockServer::start().await;
        let sse_body = concat!(
            "event: message_start\n",
            "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"usage\":{\"input_tokens\":10,\"output_tokens\":0}}}\n\n",
            "event: content_block_delta\n",
            "data: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"Hello\"}}\n\n",
            "event: content_block_delta\n",
            "data: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\" world\"}}\n\n",
            "event: message_delta\n",
            "data: {\"type\":\"message_delta\",\"usage\":{\"output_tokens\":2}}\n\n",
            "event: message_stop\n",
            "data: {\"type\":\"message_stop\"}\n\n"
        );

        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .and(header("x-api-key", "test-key"))
            .and(header("anthropic-version", "2023-06-01"))
            .and(body_partial_json(json!({"stream": true})))
            .respond_with(ResponseTemplate::new(200).set_body_raw(sse_body, "text/event-stream"))
            .mount(&server)
            .await;

        let adapter =
            AnthropicAdapter::with_base_url("test-key", Url::parse(&server.uri()).unwrap())
                .unwrap();
        let mut stream = adapter.chat_stream(sample_request()).await.unwrap();
        let mut events = Vec::new();
        while let Some(item) = stream.next().await {
            let event = item.unwrap();
            let done = matches!(event, StreamEvent::Done);
            events.push(event);
            if done {
                break;
            }
        }

        assert!(events
            .iter()
            .any(|e| matches!(e, StreamEvent::TextDelta { delta } if delta == "Hello")));
        assert!(events
            .iter()
            .any(|e| matches!(e, StreamEvent::TextDelta { delta } if delta == " world")));
        assert!(events
            .iter()
            .any(|e| matches!(e, StreamEvent::Usage { usage } if usage.input_tokens == 10)));
        assert!(events
            .iter()
            .any(|e| matches!(e, StreamEvent::Usage { usage } if usage.output_tokens == 2)));
        assert!(events.iter().any(|e| matches!(e, StreamEvent::Done)));
    }
}
