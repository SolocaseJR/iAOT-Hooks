use thiserror::Error;
#[derive(Debug, Error)]
pub enum IaotError {
    #[error("invalid pattern: {0}")]
    Pattern(String),
    #[error("invalid manifest: {0}")]
    Manifest(String),
    #[error("unsupported operation: {0}")]
    Unsupported(String),
}
