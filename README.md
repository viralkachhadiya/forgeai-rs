# forgeai-rs

`forgeai-rs` is a public, provider-agnostic Rust GenAI SDK workspace focused on:

- Unified API across providers
- Streaming-first design
- Tool/function-calling support
- Production-oriented architecture for open-source adoption

## Current status

This project is in early OSS development (`0.1.x`).

Implemented milestones:

- OpenAI adapter: real `chat` + `chat_stream` with contract tests
- Anthropic adapter: real `chat` + `chat_stream` with contract tests
- Gemini adapter: real `chat` + `chat_stream` with contract tests
- Multi-step tool-call loop orchestration (`chat_with_tools`, `chat_with_tools_streaming`)
- Provider failover router (`forgeai-router::FailoverRouter`)

## Workspace crates

- `forgeai`: ergonomic high-level SDK client
- `forgeai-core`: core request/response types and adapter traits
- `forgeai-adapter-openai`: OpenAI provider adapter
- `forgeai-adapter-anthropic`: Anthropic provider adapter
- `forgeai-adapter-gemini`: Gemini provider adapter
- `forgeai-stream`: streaming protocol helpers
- `forgeai-tools`: tool execution contracts
- `forgeai-schema`: structured output/schema helpers
- `forgeai-router`: provider routing/fallback policies (scaffold)
- `forgeai-observability`: tracing/metrics hooks (scaffold)
- `forgeai-replay`: record/replay harness (scaffold)
- `forgeai-gateway`: service layer (scaffold)

## Quick start

### 1. Add dependencies

```bash
cargo add forgeai --features openai
cargo add forgeai-adapter-openai
```

### 2. Set environment variables

```bash
export OPENAI_API_KEY="your-key"
# Optional:
# export OPENAI_BASE_URL="https://api.openai.com"
```

### 3. Minimal example

```rust
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
                content: "Hello from forgeai-rs".to_string(),
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

See runnable examples in:

- `examples/quickstart`
- `examples/streaming`
- `examples/tools`
- `examples/structured-output`
- `examples/tool-loop`
- `examples/failover-router`

## Provider environment variables

- OpenAI: `OPENAI_API_KEY`, optional `OPENAI_BASE_URL`
- Anthropic: `ANTHROPIC_API_KEY`, optional `ANTHROPIC_BASE_URL`
- Gemini: `GEMINI_API_KEY`, optional `GEMINI_BASE_URL`

## Advanced features

### Tool-call loop orchestration

`forgeai::Client` supports automatic tool-call execution loops:

- `chat_with_tools(...)`
- `chat_with_tools_streaming(...)`

These methods repeatedly:

1. generate model output
2. execute returned tool calls
3. append tool output to conversation
4. re-run generation until final answer or max-iteration limit

### Failover routing

`forgeai-router` provides `FailoverRouter` implementing `ChatAdapter`.
It tries providers in order and fails over on retryable errors:

- `ForgeError::Transport`
- `ForgeError::Provider`
- `ForgeError::RateLimited`

## Development

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

For adapter-focused contract tests:

```bash
cargo test -p forgeai-adapter-openai
cargo test -p forgeai-adapter-anthropic
cargo test -p forgeai-adapter-gemini
```

## Release model

- CI runs on pull requests and pushes to `master`
- Publishing is manual via GitHub Actions workflow `release.yml`
- Crates are intended for publication on crates.io

## Roadmap

See:

- `docs/architecture/overview.md`
- `docs/architecture/roadmap.md`
- `docs/adr/0001-clean-room-migration.md`

## Contributing

Please read:

- `CONTRIBUTING.md`
- `CODE_OF_CONDUCT.md`
- `SECURITY.md`

## License

Licensed under either:

- Apache License, Version 2.0 (`LICENSE-APACHE`)
- MIT license (`LICENSE-MIT`)

at your option.
