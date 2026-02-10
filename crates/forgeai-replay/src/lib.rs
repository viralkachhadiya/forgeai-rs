use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayEntry {
    pub request: String,
    pub response: String,
}
