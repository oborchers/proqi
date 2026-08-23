//! Crossterm terminal session and event adapter.

mod browser;
mod control;
mod external;
mod input;
mod persistence;
mod runner;
mod settings;

use thiserror::Error;

use crate::ports::store::StoreError;

pub(crate) use browser::pick_session;
pub(crate) use runner::{TerminalResources, require_interactive, run};
pub(crate) use settings::load_settings;

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
    /// User configuration is malformed or unsafe.
    #[error("terminal configuration failed: {0}")]
    Config(String),
}

impl From<std::io::Error> for TerminalError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error.to_string())
    }
}
