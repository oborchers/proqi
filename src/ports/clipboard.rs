//! Native clipboard and terminal fallback boundary.

use thiserror::Error;

use crate::domain::{ContentAnnotation, DomainError, RequestId, validate_annotations};

use super::attachment::RasterImage;

/// Exact interoperable text plus validated Proqi presentation metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClipboardText {
    content: String,
    annotations: Vec<ContentAnnotation>,
}

impl ClipboardText {
    /// Construct one exact clipboard text representation.
    ///
    /// # Errors
    ///
    /// Returns an annotation error when metadata does not belong to the exact text.
    pub fn new(content: String, annotations: Vec<ContentAnnotation>) -> Result<Self, DomainError> {
        validate_annotations(&content, &annotations)?;
        Ok(Self {
            content,
            annotations,
        })
    }

    /// Construct exact plain text without Proqi metadata.
    #[must_use]
    pub fn plain(content: String) -> Self {
        Self {
            content,
            annotations: Vec::new(),
        }
    }

    /// Exact UTF-8 text exposed through the ordinary system flavor.
    #[must_use]
    pub fn content(&self) -> &str {
        &self.content
    }

    /// Validated selection-relative presentation metadata.
    #[must_use]
    pub fn annotations(&self) -> &[ContentAnnotation] {
        &self.annotations
    }

    /// Consume the value into exact text and annotations.
    #[must_use]
    pub fn into_parts(self) -> (String, Vec<ContentAnnotation>) {
        (self.content, self.annotations)
    }
}

/// Native clipboard content accepted for prompt insertion.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ClipboardContent {
    /// Exact UTF-8 text with optional verified Proqi metadata.
    Text(ClipboardText),
    /// Validated raw image pixels requiring durable materialization.
    Image(RasterImage),
}

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
    /// The platform cannot safely bind typed metadata to the native clipboard item.
    #[error("annotated clipboard copy is unsupported on this platform")]
    MetadataUnsupported,
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
    /// Clipboard image dimensions or pixels were invalid or exceeded the bound.
    #[error("clipboard image is invalid or too large")]
    InvalidImage,
}

/// Exact text clipboard operations.
pub trait Clipboard {
    /// Write exact text and metadata through a native provider.
    ///
    /// OSC 52 is permitted only when `content` has no annotations.
    ///
    /// # Errors
    ///
    /// Returns a typed non-destructive error when no safe path succeeds.
    fn write(
        &mut self,
        request_id: RequestId,
        content: &ClipboardText,
    ) -> Result<ClipboardWrite, ClipboardError>;

    /// Read exact text from the native clipboard.
    ///
    /// # Errors
    ///
    /// Returns a typed non-destructive error when native clipboard reading is unavailable.
    fn read(&mut self) -> Result<ClipboardContent, ClipboardError>;
}
