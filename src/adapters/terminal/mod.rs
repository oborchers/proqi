//! Crossterm terminal session and event adapter.

mod browser;
mod control;
mod external;
mod input;
mod integration;
mod palette;
mod path_import;
mod persistence;
mod runner;
mod settings;
mod supervisor;
mod update_lane;

use thiserror::Error;

use crate::ports::{control::ControlError, runtime::RuntimeError, store::StoreError};

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
    /// One or more teardown operations failed after every cleanup was attempted.
    #[error("terminal cleanup failed: {0}")]
    Cleanup(String),
    /// User configuration is malformed or unsafe.
    #[error("terminal configuration failed: {0}")]
    Config(String),
    /// Active-owner local control transport failed.
    #[error(transparent)]
    Control(#[from] ControlError),
    /// Runtime ownership metadata could not be updated safely.
    #[error(transparent)]
    Runtime(#[from] RuntimeError),
}

impl From<std::io::Error> for TerminalError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error.to_string())
    }
}
