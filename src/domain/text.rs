//! Terminal-independent text value types shared by domain and editor ports.

/// A logical position measured in Unicode grapheme clusters.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct TextPosition {
    /// Zero-based logical line.
    pub line: usize,
    /// Zero-based grapheme boundary within the logical line.
    pub grapheme: usize,
}

impl TextPosition {
    /// Construct a logical text position.
    #[must_use]
    pub const fn new(line: usize, grapheme: usize) -> Self {
        Self { line, grapheme }
    }
}
