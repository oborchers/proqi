//! Native clipboard and terminal fallback boundary.

use thiserror::Error;

/// Successful clipboard write path.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ClipboardWrite {
    /// Native desktop clipboard accepted the exact content.
    Native,
    /// UI lane must emit this complete OSC 52 sequence before acknowledging success.
    Osc52(Vec<u8>),
}

/// Clipboard operation failure that never implies content mutation.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum ClipboardError {
    /// No supported native or terminal clipboard path is available.
    #[error("clipboard is unavailable: {0}")]
    Unavailable(String),
    /// Content exceeds the bounded terminal fallback.
    #[error("clipboard content is too large for terminal fallback")]
    TooLarge,
    /// A clipboard process timed out.
    #[error("clipboard operation timed out")]
    TimedOut,
    /// Clipboard response was not valid text.
    #[error("clipboard content is not valid UTF-8")]
    InvalidText,
}

/// Exact text clipboard operations.
pub trait Clipboard {
    /// Write exact text through a native provider or return an OSC 52 fallback sequence.
    ///
    /// # Errors
    ///
    /// Returns a typed non-destructive error when no safe path succeeds.
    fn write(&mut self, content: &str) -> Result<ClipboardWrite, ClipboardError>;

    /// Read exact text from the native clipboard.
    ///
    /// # Errors
    ///
    /// Returns a typed non-destructive error when native clipboard reading is unavailable.
    fn read(&mut self) -> Result<String, ClipboardError>;
}
