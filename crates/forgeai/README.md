# forgeai

High-level Rust SDK client for multi-provider GenAI.

## Install

```bash
cargo add forgeai --features openai
cargo add forgeai-adapter-openai
```

## Quick start

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
                content: "Hello from forgeai".to_string(),
            }],
            temperature: Some(0.2),
            max_tokens: Some(128),
            tools: vec![],
            metadata: json!({}),
        })
        .await?;

    println!("{}", response.output_text);
    Ok(())
}
```

## Advanced APIs

- `chat_stream(...)`
- `chat_with_tools(...)`
- `chat_with_tools_streaming(...)`
