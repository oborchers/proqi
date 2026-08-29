//! Editor contract expressed without terminal-library input types.

mod change;

pub use change::{OffsetAffinity, TextChange, TextChangeError, TextChangeSet, TextCoordinateSpace};

use crate::domain::TextPosition;

/// Fixed wrapped-row distance used by accelerated editor navigation.
pub const FAST_NAVIGATION_ROWS: usize = 5;

/// A normalized text selection.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
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
    /// Five wrapped visual rows toward the beginning of the document.
    VisualJumpUp,
    /// Five wrapped visual rows toward the end of the document.
    VisualJumpDown,
    /// Beginning of the current logical line.
    LineStart,
    /// End of the current logical line.
    LineEnd,
    /// Beginning of the document.
    DocumentStart,
    /// End of the document.
    DocumentEnd,
}

/// Unit used when a pointer begins or extends a text selection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SelectionGranularity {
    /// Grapheme-granular caret placement and dragging.
    Grapheme,
    /// Complete Unicode word under the pointer.
    Word,
    /// Complete newline-delimited logical line.
    LogicalLine,
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
    /// Insert a newline with conservative Markdown list continuation.
    InsertSmartNewline {
        /// Spaces used for every list indentation level.
        indent_width: u8,
    },
    /// Indent the current list item or every selected logical line.
    Indent {
        /// Configured space indentation width.
        width: u8,
        /// Whether recognized list items receive structure-aware indentation.
        smart_lists: bool,
    },
    /// Outdent the current list item or every selected logical line.
    Outdent {
        /// Configured space indentation width.
        width: u8,
        /// Whether recognized list items may be outdented.
        smart_lists: bool,
    },
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
        /// Canonical logical position under the pointer.
        position: TextPosition,
        /// Selection unit established by the click count.
        granularity: SelectionGranularity,
        /// Whether to extend the existing selection rather than replace it.
        extend_selection: bool,
    },
    /// Extend the active pointer selection to a logical position.
    PointerDrag {
        /// Canonical logical position under the pointer.
        position: TextPosition,
    },
    /// Finish the active pointer selection gesture.
    PointerEnd,
    /// Undo one editor operation.
    Undo,
    /// Redo one editor operation.
    Redo,
}

/// One wrapped row produced by the editor's canonical text mapping.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VisualLine {
    /// First UTF-8 byte represented by this row in the snapshot content.
    pub start_byte: usize,
    /// UTF-8 byte boundary immediately after this row.
    pub end_byte: usize,
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
    /// Selected half-open terminal-cell range on this row.
    pub selected_cells: Option<CellRange>,
}

/// Half-open terminal-cell range within one rendered row.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CellRange {
    /// First selected cell.
    pub start: usize,
    /// Cell immediately after the selection.
    pub end: usize,
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
    /// Exact ordered changes from the command's input document to `snapshot.content`.
    pub changes: TextChangeSet,
    /// Latest complete state.
    pub snapshot: EditorSnapshot,
}

/// Multiline editor boundary used by application code.
pub trait Editor {
    /// Apply one normalized command.
    fn apply(&mut self, command: EditCommand) -> EditOutcome;

    /// Reflow into a new viewport without changing logical content or position.
    fn set_viewport(&mut self, viewport: TextViewport);

    /// Scroll wrapped rows without moving the logical cursor or selection.
    fn scroll_by(&mut self, rows: isize);

    /// Return the current complete state.
    fn snapshot(&self) -> EditorSnapshot;

    /// Replace all content, report that reset explicitly, and restore the nearest valid cursor.
    fn replace_content(&mut self, text: String, cursor: TextPosition) -> EditOutcome;

    /// Resolve a visible viewport cell to a logical text position.
    fn position_at_cell(&self, row: u16, column: u16) -> TextPosition;

    /// Return selected text exactly as stored, including line delimiters.
    fn selected_text(&self) -> Option<String>;
}

/// Creates isolated editor instances without exposing an implementation to UI code.
pub trait EditorFactory {
    /// Construct an editor containing exact text.
    fn create(&self, text: &str) -> Box<dyn Editor>;
}
