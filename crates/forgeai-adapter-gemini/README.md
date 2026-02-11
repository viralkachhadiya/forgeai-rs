# forgeai-adapter-gemini

Gemini adapter for `forgeai-rs` (`chat` and `chat_stream`).

## Environment

- `GEMINI_API_KEY` (required)
- `GEMINI_BASE_URL` (optional)

## Example

```rust,no_run
use forgeai::forgeai_core::{ChatRequest, Message, Role};
use forgeai::Client;
use forgeai_adapter_gemini::GeminiAdapter;
use serde_json::json;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let adapter = GeminiAdapter::from_env()?;
    let client = Client::new(Arc::new(adapter));

    let response = client
        .chat(ChatRequest {
            model: "gemini-1.5-flash".to_string(),
            messages: vec![Message {
                role: Role::User,
                content: "Give one async Rust best practice".to_string(),
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
