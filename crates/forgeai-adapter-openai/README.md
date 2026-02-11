# forgeai-adapter-openai

OpenAI adapter for `forgeai-rs` (`chat` and `chat_stream`).

## Environment

- `OPENAI_API_KEY` (required)
- `OPENAI_BASE_URL` (optional)

## Example

```rust,no_run
use forgeai::forgeai_core::{ChatRequest, Message, Role};
use forgeai::Client;
use forgeai_adapter_openai::OpenAiAdapter;
use serde_json::json;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let adapter = OpenAiAdapter::from_env()?;
    let client = Client::new(Arc::new(adapter));

    let response = client
        .chat(ChatRequest {
            model: "gpt-4o-mini".to_string(),
            messages: vec![Message {
                role: Role::User,
                content: "Summarize Rust ownership in one line".to_string(),
            }],
            temperature: Some(0.2),
            max_tokens: Some(120),
            tools: vec![],
            metadata: json!({}),
        })
        .await?;

    println!("{}", response.output_text);
    Ok(())
}
```
