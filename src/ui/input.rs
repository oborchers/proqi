//! Terminal-independent keyboard and pointer input values.

use crate::{domain::Direction, ports::editor::CursorMovement};

use super::PastePayload;

/// Mouse button after terminal-backend normalization.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PointerButton {
    /// Primary button used for all required interactions.
    Left,
    /// Middle button, retained for portable event normalization.
    Middle,
    /// Secondary button, never required by Proqi.
    Right,
}

/// Semantic pointer event kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PointerKind {
    /// Button pressed.
    Down(PointerButton),
    /// Button released.
    Up(PointerButton),
    /// Pointer moved while a button is held.
    Drag(PointerButton),
    /// Pointer moved without a button.
    Move,
    /// Scroll toward earlier content.
    ScrollUp,
    /// Scroll toward later content.
    ScrollDown,
}

/// Terminal-cell pointer location and semantic event kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PointerInput {
    /// Zero-based terminal column.
    pub column: u16,
    /// Zero-based terminal row.
    pub row: u16,
    /// Normalized pointer event.
    pub kind: PointerKind,
    /// Whether Shift requests extension of the active text or board selection.
    pub extend_selection: bool,
}

/// Normalized keys accepted by the board UI.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiKey {
    /// Request a clean application exit from any mode.
    Quit,
    /// Insert one Unicode scalar value.
    Character(char),
    /// A printable character reported with Primary for board-keymap resolution.
    PrimaryCharacter(char),
    /// Insert a line break or enter the focused thought.
    Enter,
    /// Submit the active durable thought and remove it after acceptance.
    Submit,
    /// Submit the active durable thought and keep it.
    SubmitKeep,
    /// Accept a focused authoring completion or request indentation while editing.
    Tab,
    /// Request one conservative indentation level outward.
    BackTab,
    /// Move to the previous bounded picker result.
    PickerPrevious,
    /// Move to the next bounded picker result.
    PickerNext,
    /// Return from edit mode.
    Escape,
    /// Delete the preceding grapheme.
    Backspace,
    /// Delete the following grapheme.
    Delete,
    /// Move logically or visually, optionally extending selection.
    Move {
        /// Backend-independent cursor intention.
        movement: CursorMovement,
        /// Whether to extend the active selection.
        extend_selection: bool,
    },
    /// A mode-aware vertical chord with distinct editor and board intentions.
    ///
    /// The UI mode translator resolves this before command dispatch. This lets
    /// Alt and Primary accelerate editing without changing established board
    /// or overlay navigation.
    EditNavigation {
        /// Movement applied while directly editing a thought.
        editor_movement: CursorMovement,
        /// Existing movement retained in board mode and overlays.
        board_movement: CursorMovement,
    },
    /// Vertical movement reported with both Primary and Shift modifiers.
    ///
    /// Board mode interprets this as thought reordering. Edit mode preserves
    /// the terminal's corresponding selection movement.
    PrimaryShiftMove {
        /// Backend-independent editor movement for this chord.
        movement: CursorMovement,
    },
    /// Select the complete thought.
    SelectAll,
    /// Delete the current logical line.
    DeleteLine,
    /// Undo in the active history scope.
    Undo,
    /// Redo in the active history scope.
    Redo,
    /// Copy the active thought or editor selection.
    Copy,
    /// Cut the active thought or editor selection after clipboard success.
    Cut,
    /// Read and paste the native clipboard.
    PasteClipboard,
    /// Duplicate the focused or selected thoughts below the source range.
    Duplicate,
}

/// One-dimensional navigation for a non-text list or menu.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ListNavigation {
    /// Select or reveal the preceding item.
    Previous,
    /// Select or reveal the following item.
    Next,
}

impl UiKey {
    /// Resolve the equivalent arrow and Vim spellings used by non-text lists.
    pub(crate) const fn list_navigation(self) -> Option<ListNavigation> {
        match self {
            Self::Move {
                movement: CursorMovement::VisualUp,
                ..
            }
            | Self::Character('k') => Some(ListNavigation::Previous),
            Self::Move {
                movement: CursorMovement::VisualDown,
                ..
            }
            | Self::Character('j') => Some(ListNavigation::Next),
            _ => None,
        }
    }

    /// Resolve the equivalent arrow and Vim spellings used by direction choosers.
    pub(crate) const fn direction(self) -> Option<Direction> {
        match self {
            Self::Move {
                movement: CursorMovement::GraphemeBack,
                ..
            }
            | Self::Character('h') => Some(Direction::Left),
            Self::Move {
                movement: CursorMovement::VisualDown,
                ..
            }
            | Self::Character('j') => Some(Direction::Down),
            Self::Move {
                movement: CursorMovement::VisualUp,
                ..
            }
            | Self::Character('k') => Some(Direction::Up),
            Self::Move {
                movement: CursorMovement::GraphemeForward,
                ..
            }
            | Self::Character('l') => Some(Direction::Right),
            _ => None,
        }
    }
}

/// Input translated from a concrete terminal backend.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UiInput {
    /// One normalized key command.
    Key(UiKey),
    /// One complete bracketed or clipboard paste.
    Paste(String),
    /// One complete paste with adapter-derived presentation provenance.
    PasteAnnotated(PastePayload),
    /// Latest terminal cell dimensions.
    Resize {
        /// Latest reported terminal width.
        width: u16,
        /// Latest reported terminal height.
        height: u16,
    },
    /// Terminal host focus returned to this pane.
    HostFocusGained,
    /// Terminal host focus left this pane.
    HostFocusLost,
    /// One normalized mouse or trackpad event.
    Pointer(PointerInput),
}

impl UiInput {
    /// Whether this input represents deliberate user interaction with Proqi.
    ///
    /// Host focus, resize, bare pointer motion, and button release are passive
    /// transport events. They must not renew activity leases or invalidate an
    /// untouched capture editor.
    #[must_use]
    pub const fn is_deliberate_interaction(&self) -> bool {
        match self {
            Self::Key(_) | Self::Paste(_) | Self::PasteAnnotated(_) => true,
            Self::Pointer(pointer) => matches!(
                pointer.kind,
                PointerKind::Down(_)
                    | PointerKind::Drag(_)
                    | PointerKind::ScrollUp
                    | PointerKind::ScrollDown
            ),
            Self::Resize { .. } | Self::HostFocusGained | Self::HostFocusLost => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ListNavigation, UiKey};
    use crate::{domain::Direction, ports::editor::CursorMovement};

    const fn movement(movement: CursorMovement) -> UiKey {
        UiKey::Move {
            movement,
            extend_selection: false,
        }
    }

    #[test]
    fn non_text_list_aliases_share_one_typed_intention() {
        for (key, expected) in [
            (movement(CursorMovement::VisualUp), ListNavigation::Previous),
            (UiKey::Character('k'), ListNavigation::Previous),
            (movement(CursorMovement::VisualDown), ListNavigation::Next),
            (UiKey::Character('j'), ListNavigation::Next),
        ] {
            assert_eq!(key.list_navigation(), Some(expected));
        }
        for key in [UiKey::Character('h'), UiKey::Character('l'), UiKey::Delete] {
            assert_eq!(key.list_navigation(), None);
        }
    }

    #[test]
    fn four_way_aliases_share_one_typed_direction() {
        for (key, expected) in [
            (movement(CursorMovement::GraphemeBack), Direction::Left),
            (UiKey::Character('h'), Direction::Left),
            (movement(CursorMovement::VisualDown), Direction::Down),
            (UiKey::Character('j'), Direction::Down),
            (movement(CursorMovement::VisualUp), Direction::Up),
            (UiKey::Character('k'), Direction::Up),
            (movement(CursorMovement::GraphemeForward), Direction::Right),
            (UiKey::Character('l'), Direction::Right),
        ] {
            assert_eq!(key.direction(), Some(expected));
        }
        assert_eq!(UiKey::Character('J').direction(), None);
        assert_eq!(UiKey::Delete.direction(), None);
    }
}
