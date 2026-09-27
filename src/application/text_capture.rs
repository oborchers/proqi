//! Explicit text capture from a terminal selection or the clipboard.
//!
//! One policy owns the source order, the accepted text, and the feedback shared
//! by the Herdr plugin action and `thoughts capture`. The captured text becomes
//! thought content exactly as read: it is never trimmed, quoted, annotated, or
//! truncated, and a pane's scrollback is never read as a substitute.

use std::fmt;

use unicode_segmentation::UnicodeSegmentation as _;

use crate::ports::clipboard::{Clipboard, ClipboardContent, ClipboardError};

/// Largest thought body accepted from standard input or an explicit capture.
pub const MAX_THOUGHT_INPUT_BYTES: usize = 128 * 1024;
/// Prefix of every message about a capture that stored nothing.
pub const NOTHING_CAPTURED: &str = "Nothing captured";
/// Graphemes shown in a single-line capture preview before the ellipsis.
const PREVIEW_GRAPHEMES: usize = 40;

/// Where captured text came from.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CaptureSource {
    /// The terminal selection the host supplied with the invocation.
    Selection,
    /// The native clipboard's text.
    Clipboard,
}

impl CaptureSource {
    /// Stable machine spelling used by the CLI contract.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Selection => "selection",
            Self::Clipboard => "clipboard",
        }
    }

    const fn noun(self) -> &'static str {
        match self {
            Self::Selection => "selection",
            Self::Clipboard => "clipboard",
        }
    }
}

/// Validated text that may become exactly one thought.
#[derive(Clone, Eq, PartialEq)]
pub struct CapturedText {
    source: CaptureSource,
    text: String,
}

impl fmt::Debug for CapturedText {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CapturedText")
            .field("source", &self.source)
            .field("bytes", &self.text.len())
            .finish()
    }
}

impl CapturedText {
    /// Accept text from one source when it can become a thought unchanged.
    ///
    /// # Errors
    ///
    /// Returns [`CaptureError::Empty`] for empty or whitespace-only text and
    /// [`CaptureError::TooLarge`] above [`MAX_THOUGHT_INPUT_BYTES`].
    pub fn new(source: CaptureSource, text: String) -> Result<Self, CaptureError> {
        if text.trim().is_empty() {
            return Err(CaptureError::Empty { source });
        }
        if text.len() > MAX_THOUGHT_INPUT_BYTES {
            return Err(CaptureError::TooLarge {
                source,
                bytes: text.len(),
            });
        }
        Ok(Self { source, text })
    }

    /// Source the text came from.
    #[must_use]
    pub const fn source(&self) -> CaptureSource {
        self.source
    }

    /// Exact captured text.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Perceived characters, counted as the paste fold counts them.
    #[must_use]
    pub fn characters(&self) -> usize {
        self.text.graphemes(true).count()
    }

    /// Single-line, control-free preview of at most forty graphemes.
    #[must_use]
    pub fn preview(&self) -> String {
        let mut preview = String::new();
        let mut shown = 0;
        let mut pending_space = false;
        for grapheme in self.text.graphemes(true) {
            if grapheme.chars().all(char::is_whitespace) {
                pending_space = !preview.is_empty();
                continue;
            }
            let visible: String = grapheme.chars().filter(|c| is_visible(*c)).collect();
            if visible.is_empty() {
                continue;
            }
            let needed = usize::from(pending_space) + 1;
            if shown + needed > PREVIEW_GRAPHEMES {
                preview.push('…');
                return preview;
            }
            if pending_space {
                preview.push(' ');
            }
            preview.push_str(&visible);
            shown += needed;
            pending_space = false;
        }
        preview
    }

    /// One-line success message for a host notification.
    #[must_use]
    pub fn confirmation(&self) -> String {
        let characters = self.characters();
        let unit = if characters == 1 {
            "character"
        } else {
            "characters"
        };
        format!(
            "Captured to Proqi: \"{}\" ({characters} {unit})",
            self.preview()
        )
    }
}

/// Whether a character may appear in a one-line preview: controls and the
/// bidirectional embedding, override, and isolate formats are dropped, so a
/// preview can neither break the line nor reorder the notification around it.
fn is_visible(character: char) -> bool {
    !character.is_control()
        && !matches!(character, '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}')
}

/// Why nothing was captured.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CaptureError {
    /// The source held no text, or only whitespace.
    Empty {
        /// Source that was empty.
        source: CaptureSource,
    },
    /// The clipboard held content that is not text, such as an image.
    NoText,
    /// The text exceeds the thought input limit.
    TooLarge {
        /// Source of the oversized text.
        source: CaptureSource,
        /// Exact UTF-8 length of the rejected text.
        bytes: usize,
    },
    /// The clipboard could not be read.
    Clipboard(ClipboardError),
}

impl fmt::Display for CaptureError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty { source } => {
                write!(
                    formatter,
                    "Nothing captured: the {} is empty",
                    source.noun()
                )
            }
            Self::NoText => write!(
                formatter,
                "{NOTHING_CAPTURED}: the clipboard holds no text (for example an image)"
            ),
            Self::TooLarge { source, bytes } => write!(
                formatter,
                "{NOTHING_CAPTURED}: the {} has {bytes} bytes, more than the {MAX_THOUGHT_INPUT_BYTES}-byte thought limit",
                source.noun()
            ),
            Self::Clipboard(error) => {
                write!(
                    formatter,
                    "Nothing captured: the clipboard could not be read ({error})"
                )
            }
        }
    }
}

impl std::error::Error for CaptureError {}

/// Capture the host-supplied selection, or else the clipboard's text.
///
/// A nonempty selection always wins because the user selected it deliberately,
/// so a whitespace-only selection is rejected as empty instead of falling back.
/// The clipboard is read only without a selection.
///
/// # Errors
///
/// Returns the [`CaptureError`] explaining why nothing can be captured.
pub fn capture_text(
    selection: Option<String>,
    clipboard: &mut (impl Clipboard + ?Sized),
) -> Result<CapturedText, CaptureError> {
    match selection.filter(|text| !text.is_empty()) {
        Some(text) => CapturedText::new(CaptureSource::Selection, text),
        None => clipboard_text(clipboard),
    }
}

/// Capture the clipboard's exact text without its Proqi presentation metadata.
///
/// # Errors
///
/// Returns the [`CaptureError`] explaining why nothing can be captured.
pub fn clipboard_text(
    clipboard: &mut (impl Clipboard + ?Sized),
) -> Result<CapturedText, CaptureError> {
    match clipboard.read() {
        Ok(ClipboardContent::Text(text)) => {
            let (content, _annotations) = text.into_parts();
            CapturedText::new(CaptureSource::Clipboard, content)
        }
        Ok(ClipboardContent::Image(_))
        | Err(ClipboardError::InvalidImage | ClipboardError::InvalidText) => {
            Err(CaptureError::NoText)
        }
        Err(error) => Err(CaptureError::Clipboard(error)),
    }
}

#[cfg(test)]
mod tests;
