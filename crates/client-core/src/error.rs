use std::fmt;

use crate::JobErrorKind;

/// A host-safe failure category plus the diagnostic preserved for CLI users.
#[derive(Debug)]
pub struct JobError {
    pub kind: JobErrorKind,
    pub message: String,
}

impl JobError {
    pub fn resource(message: impl Into<String>) -> Self {
        Self {
            kind: JobErrorKind::ResourceExhausted,
            message: message.into(),
        }
    }
}

impl fmt::Display for JobError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for JobError {}
