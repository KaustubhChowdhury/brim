//! Structured, user-safe errors returned across the Rust/TypeScript boundary.
//!
//! `message` is always safe to show to the user. `detail` carries developer
//! diagnostics (paths, OS error text) and is only sent to the frontend in debug
//! builds; it is always written to the local log.

use serde::Serialize;
use std::fmt;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppError {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

pub type AppResult<T> = Result<T, AppError>;

impl AppError {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self { code: code.to_string(), message: message.into(), detail: None }
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        let detail = detail.into();
        log::warn!("[{}] {} :: {}", self.code, self.message, detail);
        if cfg!(debug_assertions) {
            self.detail = Some(detail);
        }
        self
    }

    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new("invalid_input", message)
    }

    pub fn not_found(what: &str) -> Self {
        Self::new("not_found", format!("{what} could not be found. It may have been removed."))
    }

    pub fn internal(detail: impl Into<String>) -> Self {
        Self::new("internal", "Something went wrong. The rest of the dock is unaffected.").with_detail(detail)
    }

    pub fn native(message: impl Into<String>, detail: impl Into<String>) -> Self {
        Self::new("native", message).with_detail(detail)
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)?;
        if let Some(d) = &self.detail {
            write!(f, " ({d})")?;
        }
        Ok(())
    }
}

impl std::error::Error for AppError {}

impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self {
        AppError::new("storage", "Local storage error. Your other data is unaffected.").with_detail(e.to_string())
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError::new("io", "A file operation failed.").with_detail(e.to_string())
    }
}

impl From<windows::core::Error> for AppError {
    fn from(e: windows::core::Error) -> Self {
        AppError::new("native", "A Windows operation failed.").with_detail(format!("{} ({:#x})", e.message(), e.code().0))
    }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        AppError::new("invalid_input", "The data provided was not valid.").with_detail(e.to_string())
    }
}

impl From<tauri::Error> for AppError {
    fn from(e: tauri::Error) -> Self {
        AppError::new("native", "A window operation failed.").with_detail(e.to_string())
    }
}
