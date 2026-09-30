#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum ClientError {
    #[error("{detail}")]
    InvalidInput { detail: String },
    #[error("{detail}")]
    Operation { detail: String },
}

pub type Result<T> = std::result::Result<T, ClientError>;

pub fn invalid(message: impl Into<String>) -> ClientError {
    ClientError::InvalidInput {
        detail: message.into(),
    }
}

pub fn operation(error: impl std::fmt::Display) -> ClientError {
    ClientError::Operation {
        detail: error.to_string(),
    }
}
