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
pub struct GeminiAdapter {
    pub api_key: String,
    pub base_url: Url,
    pub api_version: String,
    client: HttpClient,
}

impl GeminiAdapter {
    pub fn new(api_key: impl Into<String>) -> Result<Self, ForgeError> {
        let base_url = Url::parse("https://generativelanguage.googleapis.com")
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
            api_version: "v1beta".to_string(),
            client,
        })
    }

    pub fn from_env() -> Result<Self, ForgeError> {
        let api_key = env::var("GEMINI_API_KEY").map_err(|_| ForgeError::Authentication)?;
        match env::var("GEMINI_BASE_URL") {
            Ok(raw) => {
                let base_url = Url::parse(&raw)
                    .map_err(|e| ForgeError::Validation(format!("invalid GEMINI_BASE_URL: {e}")))?;
                Self::with_base_url(api_key, base_url)
            }
            Err(_) => Self::new(api_key),
        }
    }

    fn endpoint_url(&self, model: &str, stream: bool) -> Result<Url, ForgeError> {
        let action = if stream {
            "streamGenerateContent"
        } else {
            "generateContent"
        };
        let mut url = self
            .base_url
            .join(&format!(
                "{}/models/{}:{}",
                self.api_version,
                model.trim(),
                action
            ))
            .map_err(|e| ForgeError::Internal(format!("failed to construct endpoint url: {e}")))?;
        {
            let mut qp = url.query_pairs_mut();
            qp.append_pair("key", &self.api_key);
            if stream {
                qp.append_pair("alt", "sse");
            }
        }
        Ok(url)
    }
}

#[async_trait]
impl ChatAdapter for GeminiAdapter {
    fn info(&self) -> AdapterInfo {
        AdapterInfo {
            name: "gemini".to_string(),
            base_url: Url::parse("https://generativelanguage.googleapis.com").ok(),
            capabilities: CapabilityMatrix {
                streaming: true,
                tools: true,
                structured_output: true,
                multimodal_input: true,
                citations: true,
            },
        }
    }

    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse, ForgeError> {
        let url = self.endpoint_url(&request.model, false)?;
        let model = request.model.clone();
        let response = self
            .client
            .post(url)
            .json(&build_generate_body(request))
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
        parse_chat_response(model, payload)
    }

    async fn chat_stream(
        &self,
        request: ChatRequest,
    ) -> Result<StreamResult<StreamEvent>, ForgeError> {
        let url = self.endpoint_url(&request.model, true)?;
        let response = self
            .client
            .post(url)
            .json(&build_generate_body(request))
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
                            if matches!(event, StreamEvent::Done) {
                                saw_done = true;
                            }
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
                            if matches!(event, StreamEvent::Done) {
                                saw_done = true;
                            }
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

fn build_generate_body(request: ChatRequest) -> Value {
    let mut body = Map::new();
    if let Some(temperature) = request.temperature {
        body.insert(
            "generationConfig".to_string(),
            json!({
                "temperature": temperature,
                "maxOutputTokens": request.max_tokens
            }),
        );
    } else if let Some(max_tokens) = request.max_tokens {
        body.insert(
            "generationConfig".to_string(),
            json!({
                "maxOutputTokens": max_tokens
            }),
        );
    }

    let mut contents = Vec::new();
    let mut system_chunks = Vec::new();
    for message in request.messages {
        if matches!(message.role, Role::System) {
            system_chunks.push(message.content);
            continue;
        }
        let role = if matches!(message.role, Role::Assistant) {
            "model"
        } else {
            "user"
        };
        contents.push(json!({
            "role": role,
            "parts": [{ "text": message.content }]
        }));
    }
    body.insert("contents".to_string(), Value::Array(contents));

    if !system_chunks.is_empty() {
        body.insert(
            "systemInstruction".to_string(),
            json!({
                "parts": [{
                    "text": system_chunks.join("\n\n")
                }]
            }),
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
                            "functionDeclarations": [{
                                "name": tool.name,
                                "description": tool.description,
                                "parameters": tool.input_schema
                            }]
                        })
                    })
                    .collect(),
            ),
        );
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

fn parse_chat_response(model: String, payload: Value) -> Result<ChatResponse, ForgeError> {
    let output_text = extract_text_from_payload(&payload);
    let tool_calls = extract_tool_calls_from_payload(&payload);
    let usage = extract_usage(payload.get("usageMetadata"));

    Ok(ChatResponse {
        id: payload
            .get("responseId")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        model,
        output_text,
        tool_calls,
        usage,
    })
}

fn extract_text_from_payload(payload: &Value) -> String {
    payload
        .get("candidates")
        .and_then(Value::as_array)
        .map(|candidates| {
            candidates
                .iter()
                .flat_map(|candidate| {
                    candidate
                        .get("content")
                        .and_then(|c| c.get("parts"))
                        .and_then(Value::as_array)
                        .cloned()
                        .unwrap_or_default()
                })
                .filter_map(|part| {
                    part.get("text")
                        .and_then(Value::as_str)
                        .map(ToString::to_string)
                })
                .collect::<Vec<_>>()
                .join("")
        })
        .unwrap_or_default()
}

fn extract_tool_calls_from_payload(payload: &Value) -> Vec<ToolCall> {
    payload
        .get("candidates")
        .and_then(Value::as_array)
        .map(|candidates| {
            candidates
                .iter()
                .flat_map(|candidate| {
                    candidate
                        .get("content")
                        .and_then(|c| c.get("parts"))
                        .and_then(Value::as_array)
                        .cloned()
                        .unwrap_or_default()
                })
                .filter_map(|part| {
                    let function_call = part.get("functionCall")?;
                    Some(ToolCall {
                        id: function_call
                            .get("id")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        name: function_call
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        arguments: function_call.get("args").cloned().unwrap_or(Value::Null),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn extract_usage(raw: Option<&Value>) -> Option<Usage> {
    let usage = raw?;
    let input_tokens = usage
        .get("promptTokenCount")
        .and_then(Value::as_u64)
        .unwrap_or(0) as u32;
    let output_tokens = usage
        .get("candidatesTokenCount")
        .and_then(Value::as_u64)
        .unwrap_or(0) as u32;
    let total_tokens = usage
        .get("totalTokenCount")
        .and_then(Value::as_u64)
        .map(|v| v as u32)
        .unwrap_or_else(|| input_tokens.saturating_add(output_tokens));
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
    let text = extract_text_from_payload(&value);
    if !text.is_empty() {
        events.push(StreamEvent::TextDelta { delta: text });
    }

    for tool_call in extract_tool_calls_from_payload(&value) {
        events.push(StreamEvent::ToolCallDelta {
            call_id: tool_call.id,
            delta: json!({
                "name": tool_call.name,
                "arguments": tool_call.arguments
            }),
        });
    }

    if let Some(usage) = extract_usage(value.get("usageMetadata")) {
        events.push(StreamEvent::Usage { usage });
    }

    if value
        .get("candidates")
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .and_then(|c| c.get("finishReason"))
        .is_some()
    {
        events.push(StreamEvent::Done);
    }

    Ok(events)
}

#[cfg(test)]
mod tests {
    use super::*;
    use forgeai_core::{ChatRequest, Message, Role};
    use futures_util::StreamExt;
    use wiremock::matchers::{body_partial_json, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn sample_request() -> ChatRequest {
        ChatRequest {
            model: "gemini-1.5-flash".to_string(),
            messages: vec![Message {
                role: Role::User,
                content: "Say hello".to_string(),
            }],
            temperature: Some(0.2),
            max_tokens: Some(64),
            tools: vec![],
            metadata: json!({}),
        }
    }

    #[tokio::test]
    async fn chat_contract_parses_response_and_usage() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1beta/models/gemini-1.5-flash:generateContent"))
            .and(query_param("key", "test-key"))
            .and(body_partial_json(json!({"contents": [{"role":"user"}]})))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "responseId": "resp_123",
                "candidates": [{
                    "content": {
                        "parts": [{"text":"Hello from Gemini"}]
                    }
                }],
                "usageMetadata": {
                    "promptTokenCount": 9,
                    "candidatesTokenCount": 4,
                    "totalTokenCount": 13
                }
            })))
            .mount(&server)
            .await;

        let adapter =
            GeminiAdapter::with_base_url("test-key", Url::parse(&server.uri()).unwrap()).unwrap();
        let response = adapter.chat(sample_request()).await.unwrap();

        assert_eq!(response.id, "resp_123");
        assert_eq!(response.model, "gemini-1.5-flash");
        assert_eq!(response.output_text, "Hello from Gemini");
        assert_eq!(response.usage.unwrap().total_tokens, 13);
    }

    #[tokio::test]
    async fn chat_stream_contract_parses_sse_events() {
        let server = MockServer::start().await;
        let sse_body = concat!(
            "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"Hello\"}]}}]}\n\n",
            "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\" world\"}]}}]}\n\n",
            "data: {\"usageMetadata\":{\"promptTokenCount\":9,\"candidatesTokenCount\":2,\"totalTokenCount\":11},\"candidates\":[{\"finishReason\":\"STOP\"}]}\n\n"
        );

        Mock::given(method("POST"))
            .and(path(
                "/v1beta/models/gemini-1.5-flash:streamGenerateContent",
            ))
            .and(query_param("key", "test-key"))
            .and(query_param("alt", "sse"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(sse_body, "text/event-stream"))
            .mount(&server)
            .await;

        let adapter =
            GeminiAdapter::with_base_url("test-key", Url::parse(&server.uri()).unwrap()).unwrap();
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
            .any(|e| matches!(e, StreamEvent::Usage { usage } if usage.total_tokens == 11)));
        assert!(events.iter().any(|e| matches!(e, StreamEvent::Done)));
    }
}
