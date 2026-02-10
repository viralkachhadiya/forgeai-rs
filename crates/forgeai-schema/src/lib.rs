use serde::Serialize;
use serde_json::{json, Value};

pub fn type_schema<T: Serialize>() -> Value {
    // Placeholder until schemars integration is added.
    let _ = std::marker::PhantomData::<T>;
    json!({"type": "object"})
}
