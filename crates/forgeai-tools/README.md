# forgeai-tools

Tool execution contracts for `forgeai-rs`.

## Example

```rust
use forgeai_tools::{ToolError, ToolExecutor};
use serde_json::{json, Value};

struct DemoTools;

impl ToolExecutor for DemoTools {
    fn call(&self, name: &str, input: Value) -> Result<Value, ToolError> {
        match name {
            "echo" => Ok(json!({ "echo": input })),
            _ => Err(ToolError::NotFound(name.to_string())),
        }
    }
}
```
