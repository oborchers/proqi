//! Crossterm terminal session and event adapter.

mod control;
mod input;
mod runner;

use thiserror::Error;

use crate::ports::store::StoreError;

pub(crate) use runner::{TerminalResources, require_interactive, run};

/// Failure while owning or driving the interactive terminal.
#[derive(Debug, Error)]
pub enum TerminalError {
    /// Terminal setup, input, drawing, or restoration failed.
    #[error("terminal I/O failed: {0}")]
    Io(String),
    /// The ordered persistence lane failed.
    #[error(transparent)]
    Store(#[from] StoreError),
    /// A bounded worker lane ended unexpectedly.
    #[error("terminal worker failed: {0}")]
    Worker(&'static str),
}

impl From<std::io::Error> for TerminalError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error.to_string())
    }
}
