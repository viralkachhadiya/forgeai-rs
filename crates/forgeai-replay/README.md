# forgeai-replay

Record/replay test harness primitives for `forgeai-rs`.

## Example

```rust
use forgeai_replay::ReplayEntry;

fn sample() -> ReplayEntry {
    ReplayEntry {
        request: "{\"input\":\"hello\"}".to_string(),
        response: "{\"output\":\"world\"}".to_string(),
    }
}
```
