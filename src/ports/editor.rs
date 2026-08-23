//! Editor contract expressed without terminal-library input types.

/// A logical position measured in Unicode grapheme clusters.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
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

/// A normalized text selection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TextSelection {
    /// Earlier end of the selection.
    pub start: TextPosition,
    /// Later end of the selection.
    pub end: TextPosition,
}

/// Size of the text viewport in terminal cells.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TextViewport {
    /// Available terminal columns.
    pub width: u16,
    /// Available terminal rows.
    pub height: u16,
}

impl TextViewport {
    /// Construct a nonzero viewport. Zero dimensions are clamped to one.
    #[must_use]
    pub const fn new(width: u16, height: u16) -> Self {
        Self {
            width: if width == 0 { 1 } else { width },
            height: if height == 0 { 1 } else { height },
        }
    }
}

impl Default for TextViewport {
    fn default() -> Self {
        Self::new(80, 24)
    }
}

/// Logical or visual cursor movement.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CursorMovement {
    /// Previous grapheme cluster.
    GraphemeBack,
    /// Next grapheme cluster.
    GraphemeForward,
    /// Beginning of the previous Unicode word.
    WordBack,
    /// End of the current word or beginning of the next word.
    WordForward,
    /// Previous wrapped visual row.
    VisualUp,
    /// Next wrapped visual row.
    VisualDown,
    /// Beginning of the current logical line.
    LineStart,
    /// End of the current logical line.
    LineEnd,
    /// Beginning of the document.
    DocumentStart,
    /// End of the document.
    DocumentEnd,
}

/// A normalized editor command.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EditCommand {
    /// Insert one character.
    InsertChar(char),
    /// Insert an arbitrary payload as one semantic operation.
    Paste(String),
    /// Insert a line feed.
    InsertNewline,
    /// Delete the grapheme before the cursor or the selection.
    DeleteBack,
    /// Delete the grapheme after the cursor or the selection.
    DeleteForward,
    /// Delete the current newline-delimited logical line.
    DeleteLogicalLine,
    /// Move the cursor, optionally extending the selection.
    Move {
        /// Movement to apply.
        movement: CursorMovement,
        /// Whether the existing or newly created selection is extended.
        extend_selection: bool,
    },
    /// Select the whole document.
    SelectAll,
    /// Clear the active selection without moving the cursor.
    ClearSelection,
    /// Place the cursor at a logical position.
    SetCursor {
        /// Requested position, clamped to valid content.
        position: TextPosition,
        /// Whether to extend the selection from its anchor.
        extend_selection: bool,
    },
    /// Begin a pointer selection at a viewport cell.
    PointerStart {
        /// Zero-based viewport row.
        row: u16,
        /// Zero-based viewport column.
        column: u16,
    },
    /// Extend a pointer selection to a viewport cell.
    PointerDrag {
        /// Zero-based viewport row.
        row: u16,
        /// Zero-based viewport column.
        column: u16,
    },
    /// Undo one editor operation.
    Undo,
    /// Redo one editor operation.
    Redo,
}

/// One wrapped row produced by the editor's canonical text mapping.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VisualLine {
    /// Source logical line.
    pub logical_line: usize,
    /// First grapheme included in this row.
    pub start_grapheme: usize,
    /// Grapheme boundary immediately after this row.
    pub end_grapheme: usize,
    /// Rendered cell width.
    pub cell_width: usize,
    /// Exact visible text for this row, without the line delimiter.
    pub text: String,
}

/// Serializable application-facing view of transient editor state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EditorSnapshot {
    /// Exact document content.
    pub content: String,
    /// Logical cursor position.
    pub cursor: TextPosition,
    /// Normalized active selection, if any.
    pub selection: Option<TextSelection>,
    /// Current viewport.
    pub viewport: TextViewport,
    /// First visible wrapped row.
    pub scroll_row: usize,
    /// All wrapped rows, used by layout and hit testing.
    pub visual_lines: Vec<VisualLine>,
}

/// Result of applying one normalized editor command.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EditOutcome {
    /// Whether document content changed.
    pub content_changed: bool,
    /// Latest complete state.
    pub snapshot: EditorSnapshot,
}

/// Multiline editor boundary used by application code.
pub trait Editor {
    /// Apply one normalized command.
    fn apply(&mut self, command: EditCommand) -> EditOutcome;

    /// Reflow into a new viewport without changing logical content or position.
    fn set_viewport(&mut self, viewport: TextViewport);

    /// Return the current complete state.
    fn snapshot(&self) -> EditorSnapshot;

    /// Replace all content and restore the nearest valid logical cursor.
    fn replace_content(&mut self, text: String, cursor: TextPosition);

    /// Resolve a visible viewport cell to a logical text position.
    fn position_at_cell(&self, row: u16, column: u16) -> TextPosition;

    /// Return selected text exactly as stored, including line delimiters.
    fn selected_text(&self) -> Option<String>;
}
