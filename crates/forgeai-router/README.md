# forgeai-router

Routing and failover helpers for `forgeai-rs`.

## FailoverRouter

`FailoverRouter` implements `ChatAdapter` and retries across adapters in order for retryable failures.

## Example

```rust,no_run
use forgeai::forgeai_core::ChatAdapter;
use forgeai_adapter_anthropic::AnthropicAdapter;
use forgeai_adapter_openai::OpenAiAdapter;
use forgeai_router::FailoverRouter;
use std::sync::Arc;

fn build_router() -> Result<FailoverRouter, Box<dyn std::error::Error>> {
    let a: Arc<dyn ChatAdapter> = Arc::new(OpenAiAdapter::from_env()?);
    let b: Arc<dyn ChatAdapter> = Arc::new(AnthropicAdapter::from_env()?);
    Ok(FailoverRouter::new(vec![a, b])?)
}
```
