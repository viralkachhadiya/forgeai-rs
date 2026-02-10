use serde_json::Value;

#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("tool not found: {0}")]
    NotFound(String),
    #[error("tool execution failed: {0}")]
    Execution(String),
}

pub trait ToolExecutor: Send + Sync {
    fn call(&self, name: &str, input: Value) -> Result<Value, ToolError>;
}
