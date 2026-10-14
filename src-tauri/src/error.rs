use serde::Serialize;

/// One error type for every layer. Messages are safe to show and to log: they never
/// contain secret values, decrypted metadata, or vault credentials.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    Validation(String),
    #[error("entry not found")]
    NotFound,
    #[error("this entry was changed elsewhere; reload it before saving")]
    Conflict,
    #[error("the vault is locked")]
    Locked,
    #[error("this window is not allowed to call {0}")]
    Forbidden(String),
    #[error("running this command needs explicit approval for the saved version")]
    ApprovalRequired,
    #[error("vault storage is unavailable: {0}")]
    VaultUnavailable(String),
    #[error("storage error")]
    Storage(String),
    #[error("clipboard error: {0}")]
    Clipboard(String),
    #[error("execution error: {0}")]
    Execution(String),
    #[error("{0}")]
    Other(String),
}

impl AppError {
    pub fn code(&self) -> &'static str {
        match self {
            AppError::Validation(_) => "validation",
            AppError::NotFound => "not-found",
            AppError::Conflict => "conflict",
            AppError::Locked => "locked",
            AppError::Forbidden(_) => "forbidden",
            AppError::ApprovalRequired => "approval-required",
            AppError::VaultUnavailable(_) => "vault-unavailable",
            AppError::Storage(_) => "storage",
            AppError::Clipboard(_) => "clipboard",
            AppError::Execution(_) => "execution",
            AppError::Other(_) => "error",
        }
    }

    pub fn storage<E: std::fmt::Display>(error: E) -> Self {
        AppError::Storage(error.to_string())
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(value: rusqlite::Error) -> Self {
        AppError::Storage(value.to_string())
    }
}

impl From<std::io::Error> for AppError {
    fn from(value: std::io::Error) -> Self {
        AppError::Other(value.to_string())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(value: serde_json::Error) -> Self {
        AppError::Storage(value.to_string())
    }
}

/// The webview receives the human-readable message only; the code is kept for tests and
/// for the exit codes used by the CLI.
impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;
