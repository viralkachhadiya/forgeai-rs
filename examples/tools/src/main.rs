use forgeai_tools::ToolExecutor;
use serde_json::{json, Value};

struct DemoTools;

impl ToolExecutor for DemoTools {
    fn call(&self, name: &str, input: Value) -> Result<Value, forgeai_tools::ToolError> {
        match name {
            "echo" => Ok(json!({ "echo": input })),
            _ => Err(forgeai_tools::ToolError::NotFound(name.to_string())),
        }
    }
}

fn main() {
    let out = DemoTools.call("echo", json!({ "message": "hello" })).unwrap();
    println!("{out}");
}
