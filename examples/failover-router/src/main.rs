use forgeai::forgeai_core::{ChatRequest, ChatResponse, Message, Role};
use forgeai::Client;
use forgeai_adapter_anthropic::AnthropicAdapter;
use forgeai_adapter_openai::OpenAiAdapter;
use forgeai_router::FailoverRouter;
use serde_json::json;
use std::sync::Arc;
use url::Url;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Example routing order: OpenAI first, Anthropic fallback.
    // Replace with your own key/base URL strategy in production.
    let openai = Arc::new(OpenAiAdapter::from_env()?);

    // Demo-only placeholder uses the same env key to construct the fallback adapter.
    // In production use ANTHROPIC_API_KEY + AnthropicAdapter::from_env().
    let anthropic = Arc::new(AnthropicAdapter::with_base_url(
        std::env::var("OPENAI_API_KEY")?,
        Url::parse("https://api.anthropic.com")?,
    )?);

    let router = FailoverRouter::new(vec![openai, anthropic])?;
    let client = Client::new(Arc::new(router));

    let response: ChatResponse = client
        .chat(ChatRequest {
            model: "gpt-4o-mini".to_string(),
            messages: vec![Message {
                role: Role::User,
                content: "Give me a one-line Rust tip".to_string(),
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
