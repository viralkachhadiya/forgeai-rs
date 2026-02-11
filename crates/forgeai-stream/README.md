# forgeai-stream

Streaming protocol helpers for `forgeai-rs`.

This crate currently re-exports `forgeai_core::StreamEvent`.

## Example

```rust
use forgeai_stream::StreamEvent;

fn handle(event: StreamEvent) {
    match event {
        StreamEvent::TextDelta { delta } => println!("{delta}"),
        StreamEvent::Usage { usage } => println!("total tokens: {}", usage.total_tokens),
        StreamEvent::ToolCallDelta { .. } => {}
        StreamEvent::Done => {}
    }
}
```
