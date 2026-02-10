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
pub struct OpenAiAdapter {
    pub api_key: String,
    pub base_url: Url,
    client: HttpClient,
}

impl OpenAiAdapter {
    pub fn new(api_key: impl Into<String>) -> Result<Self, ForgeError> {
        let base_url = Url::parse("https://api.openai.com")
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
            client,
        })
    }

    pub fn from_env() -> Result<Self, ForgeError> {
        let api_key = env::var("OPENAI_API_KEY").map_err(|_| ForgeError::Authentication)?;
        match env::var("OPENAI_BASE_URL") {
            Ok(raw) => {
                let base_url = Url::parse(&raw)
                    .map_err(|e| ForgeError::Validation(format!("invalid OPENAI_BASE_URL: {e}")))?;
                Self::with_base_url(api_key, base_url)
            }
            Err(_) => Self::new(api_key),
        }
    }

    fn chat_completions_url(&self) -> Result<Url, ForgeError> {
        self.base_url
            .join("v1/chat/completions")
            .map_err(|e| ForgeError::Internal(format!("failed to construct endpoint url: {e}")))
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
        let response = self
            .client
            .post(self.chat_completions_url()?)
            .bearer_auth(&self.api_key)
            .json(&build_chat_body(request, false))
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
            .post(self.chat_completions_url()?)
            .bearer_auth(&self.api_key)
            .json(&build_chat_body(request, true))
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

            while let Some(chunk) = bytes.next().await {
                let chunk = chunk.map_err(|e| ForgeError::Transport(format!("stream chunk error: {e}")))?;
                let chunk_text = std::str::from_utf8(&chunk)
                    .map_err(|e| ForgeError::Transport(format!("invalid utf8 stream chunk: {e}")))?;
                buffer.push_str(chunk_text);

                while let Some(line_end) = buffer.find('\n') {
                    let mut line = buffer[..line_end].to_string();
                    buffer.drain(..=line_end);
                    if line.ends_with('\r') {
                        line.pop();
                    }
                    if line.trim().is_empty() {
                        continue;
                    }
                    if let Some(data) = line.strip_prefix("data:") {
                        let payload = data.trim();
                        if payload == "[DONE]" {
                            saw_done = true;
                            yield StreamEvent::Done;
                            continue;
                        }
                        for event in parse_stream_payload(payload)? {
                            yield event;
                        }
                    }
                }
            }

            if !buffer.trim().is_empty() {
                let line = buffer.trim();
                if let Some(data) = line.strip_prefix("data:") {
                    let payload = data.trim();
                    if payload == "[DONE]" {
                        saw_done = true;
                        yield StreamEvent::Done;
                    } else {
                        for event in parse_stream_payload(payload)? {
                            yield event;
                        }
                    }
                }
            }

            if !saw_done {
                yield StreamEvent::Done;
            }
        };

        Ok(Box::pin(stream))
    }
}

fn build_chat_body(request: ChatRequest, stream: bool) -> Value {
    let mut body = Map::new();
    body.insert("model".to_string(), Value::String(request.model));
    body.insert(
        "messages".to_string(),
        Value::Array(
            request
                .messages
                .into_iter()
                .map(|m| {
                    json!({
                        "role": role_to_openai(&m.role),
                        "content": m.content
                    })
                })
                .collect(),
        ),
    );
    if let Some(temperature) = request.temperature {
        body.insert("temperature".to_string(), json!(temperature));
    }
    if let Some(max_tokens) = request.max_tokens {
        body.insert("max_tokens".to_string(), json!(max_tokens));
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
                            "type": "function",
                            "function": {
                                "name": tool.name,
                                "description": tool.description,
                                "parameters": tool.input_schema,
                            }
                        })
                    })
                    .collect(),
            ),
        );
    }
    if stream {
        body.insert("stream".to_string(), Value::Bool(true));
        body.insert("stream_options".to_string(), json!({"include_usage": true}));
    }
    Value::Object(body)
}

fn role_to_openai(role: &Role) -> &'static str {
    match role {
        Role::System => "system",
        Role::User => "user",
        Role::Assistant => "assistant",
        Role::Tool => "tool",
    }
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

    let choice = payload
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first());

    let message = choice
        .and_then(|c| c.get("message"))
        .unwrap_or(&Value::Null);
    let output_text = extract_text_content(message.get("content"));
    let tool_calls = extract_tool_calls(message.get("tool_calls"));
    let usage = extract_usage(payload.get("usage"));

    Ok(ChatResponse {
        id,
        model,
        output_text,
        tool_calls,
        usage,
    })
}

fn extract_text_content(content: Option<&Value>) -> String {
    match content {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(parts)) => parts
            .iter()
            .filter_map(|part| part.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join(""),
        _ => String::new(),
    }
}

fn extract_tool_calls(raw: Option<&Value>) -> Vec<ToolCall> {
    raw.and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .map(|item| {
                    let id = item
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    let function = item.get("function").unwrap_or(&Value::Null);
                    let name = function
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    let arguments = function
                        .get("arguments")
                        .and_then(Value::as_str)
                        .and_then(|raw_args| serde_json::from_str::<Value>(raw_args).ok())
                        .unwrap_or_else(|| {
                            function.get("arguments").cloned().unwrap_or(Value::Null)
                        });
                    ToolCall {
                        id,
                        name,
                        arguments,
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

fn extract_usage(raw: Option<&Value>) -> Option<Usage> {
    let usage = raw?;
    let input_tokens = usage.get("prompt_tokens")?.as_u64()? as u32;
    let output_tokens = usage.get("completion_tokens")?.as_u64()? as u32;
    let total_tokens = usage.get("total_tokens")?.as_u64()? as u32;
    Some(Usage {
        input_tokens,
        output_tokens,
        total_tokens,
    })
}

fn parse_stream_payload(payload: &str) -> Result<Vec<StreamEvent>, ForgeError> {
    let value = serde_json::from_str::<Value>(payload)
        .map_err(|e| ForgeError::Provider(format!("invalid stream payload: {e}")))?;

    let mut events = Vec::new();
    if let Some(usage) = extract_usage(value.get("usage")) {
        events.push(StreamEvent::Usage { usage });
    }

    if let Some(choices) = value.get("choices").and_then(Value::as_array) {
        for choice in choices {
            if let Some(content) = choice
                .get("delta")
                .and_then(|d| d.get("content"))
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
            {
                events.push(StreamEvent::TextDelta {
                    delta: content.to_string(),
                });
            }

            if let Some(tool_calls) = choice
                .get("delta")
                .and_then(|d| d.get("tool_calls"))
                .and_then(Value::as_array)
            {
                for tool_call in tool_calls {
                    let call_id = tool_call
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    events.push(StreamEvent::ToolCallDelta {
                        call_id,
                        delta: tool_call.clone(),
                    });
                }
            }
        }
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
            model: "gpt-4o-mini".to_string(),
            messages: vec![Message {
                role: Role::User,
                content: "Say hello".to_string(),
            }],
            temperature: Some(0.2),
            max_tokens: Some(32),
            tools: vec![],
            metadata: json!({}),
        }
    }

    #[tokio::test]
    async fn chat_contract_parses_response_and_usage() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .and(header("authorization", "Bearer test-key"))
            .and(body_partial_json(json!({"model": "gpt-4o-mini"})))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "chatcmpl-123",
                "model": "gpt-4o-mini",
                "choices": [{
                    "index": 0,
                    "message": {"role": "assistant", "content": "Hello from OpenAI"}
                }],
                "usage": {"prompt_tokens": 10, "completion_tokens": 4, "total_tokens": 14}
            })))
            .mount(&server)
            .await;

        let adapter =
            OpenAiAdapter::with_base_url("test-key", Url::parse(&server.uri()).unwrap()).unwrap();
        let response = adapter.chat(sample_request()).await.unwrap();

        assert_eq!(response.id, "chatcmpl-123");
        assert_eq!(response.model, "gpt-4o-mini");
        assert_eq!(response.output_text, "Hello from OpenAI");
        assert_eq!(response.usage.unwrap().total_tokens, 14);
    }

    #[tokio::test]
    async fn chat_stream_contract_parses_sse_events() {
        let server = MockServer::start().await;
        let sse_body = concat!(
            "data: {\"id\":\"chatcmpl-1\",\"model\":\"gpt-4o-mini\",\"choices\":[{\"delta\":{\"content\":\"Hello\"},\"index\":0}]}\n\n",
            "data: {\"id\":\"chatcmpl-1\",\"model\":\"gpt-4o-mini\",\"choices\":[{\"delta\":{\"content\":\" world\"},\"index\":0}]}\n\n",
            "data: {\"usage\":{\"prompt_tokens\":10,\"completion_tokens\":2,\"total_tokens\":12},\"choices\":[]}\n\n",
            "data: [DONE]\n\n"
        );

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .and(header("authorization", "Bearer test-key"))
            .and(body_partial_json(json!({"stream": true})))
            .respond_with(ResponseTemplate::new(200).set_body_raw(sse_body, "text/event-stream"))
            .mount(&server)
            .await;

        let adapter =
            OpenAiAdapter::with_base_url("test-key", Url::parse(&server.uri()).unwrap()).unwrap();
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
        assert!(events.iter().any(|e| matches!(
            e,
            StreamEvent::Usage { usage } if usage.total_tokens == 12
        )));
        assert!(events.iter().any(|e| matches!(e, StreamEvent::Done)));
    }
}
