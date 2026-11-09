#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum ClientError {
    #[error("{detail}")]
    InvalidInput { detail: String },
    #[error("{detail}")]
    Operation { detail: String },
}

/// Reports registry failures as operation errors, so existing native callers
/// that match every `ClientError` case keep compiling.
pub fn registry_operation(error: impl std::fmt::Display) -> ClientError {
    let detail = error.to_string();
    let detail = if detail.contains("INSTANCE_UNSUPPORTED") {
        "Registered-device rendezvous is unavailable on this instance".into()
    } else {
        detail
    };
    ClientError::Operation { detail }
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
