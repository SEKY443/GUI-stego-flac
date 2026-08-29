//! A single error type every Tauri command returns.
//!
//! Tauri serializes a command's `Err` variant straight to the frontend, so it
//! only needs to implement [`serde::Serialize`] — there is no IPC boundary
//! subtlety beyond that. Every underlying error (`anyhow::Error`, the core
//! crate's typed errors, a plain validation message) collapses to one
//! human-readable string, which is exactly what the UI displays in an error
//! banner; nothing downstream needs to branch on error *kind*.
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct CommandError {
    pub message: String,
}

pub type CmdResult<T> = Result<T, CommandError>;

impl std::fmt::Display for CommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for CommandError {}

impl From<String> for CommandError {
    fn from(message: String) -> Self {
        CommandError { message }
    }
}

impl From<&str> for CommandError {
    fn from(message: &str) -> Self {
        CommandError {
            message: message.to_string(),
        }
    }
}

impl From<anyhow::Error> for CommandError {
    fn from(error: anyhow::Error) -> Self {
        CommandError {
            message: format!("{error:#}"),
        }
    }
}
