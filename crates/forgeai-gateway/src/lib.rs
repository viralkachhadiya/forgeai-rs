#[derive(Debug, thiserror::Error)]
pub enum GatewayError {
    #[error("not implemented")]
    NotImplemented,
}
