# forgeai-schema

Structured-output schema helpers for `forgeai-rs`.

## Example

```rust
use forgeai_schema::type_schema;
use serde::Serialize;

#[derive(Serialize)]
struct Answer {
    value: String,
    confidence: f32,
}

fn main() {
    let schema = type_schema::<Answer>();
    println!("{schema}");
}
```
