use forgeai::forgeai_core::{ChatRequest, Message, Role};
use forgeai::forgeai_tools::{ToolError, ToolExecutor};
use forgeai::{Client, ToolLoopOptions};
use forgeai_adapter_openai::OpenAiAdapter;
use serde_json::{json, Value};
use std::sync::Arc;

struct DemoTools;

impl ToolExecutor for DemoTools {
    fn call(&self, name: &str, input: Value) -> Result<Value, ToolError> {
        match name {
            "time.now" => {
                let timezone = input
                    .get("timezone")
                    .and_then(Value::as_str)
                    .unwrap_or("UTC");
                Ok(json!({
                    "timezone": timezone,
                    "time": "2026-02-10T12:00:00Z"
                }))
            }
            _ => Err(ToolError::NotFound(name.to_string())),
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let adapter = OpenAiAdapter::from_env()?;
    let client = Client::new(Arc::new(adapter));

    let request = ChatRequest {
        model: "gpt-4o-mini".to_string(),
        messages: vec![Message {
            role: Role::User,
            content: "What time is it in UTC? Use the time.now tool.".to_string(),
        }],
        temperature: Some(0.1),
        max_tokens: Some(256),
        tools: vec![],
        metadata: json!({}),
    };

    let result = client
        .chat_with_tools(request, &DemoTools, ToolLoopOptions::default())
        .await?;

    println!("Final response: {}", result.final_response.output_text);
    println!("Tool invocations: {}", result.tool_invocations.len());

    Ok(())
}
