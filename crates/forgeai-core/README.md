# forgeai-core

Core domain types and adapter traits for `forgeai-rs`.

## What this crate provides

- `ChatRequest`, `ChatResponse`, `Message`, `Role`
- `StreamEvent` and `StreamResult`
- `ChatAdapter` trait
- `ForgeError` error model

## Minimal trait implementation

```rust
use async_trait::async_trait;
use forgeai_core::{
    AdapterInfo, CapabilityMatrix, ChatAdapter, ChatRequest, ChatResponse, ForgeError,
    StreamEvent, StreamResult,
};

struct MyAdapter;

#[async_trait]
impl ChatAdapter for MyAdapter {
    fn info(&self) -> AdapterInfo {
        AdapterInfo {
            name: "my-adapter".to_string(),
            base_url: None,
            capabilities: CapabilityMatrix {
                streaming: false,
                tools: false,
                structured_output: false,
                multimodal_input: false,
                citations: false,
            },
        }
    }

    async fn chat(&self, _request: ChatRequest) -> Result<ChatResponse, ForgeError> {
        Err(ForgeError::Provider("not implemented".to_string()))
    }

    async fn chat_stream(&self, _request: ChatRequest) -> Result<StreamResult<StreamEvent>, ForgeError> {
        Err(ForgeError::Provider("not implemented".to_string()))
    }
}
```
