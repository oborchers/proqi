//! Mapping from stable action identities into existing UI intentions.

use crate::{
    ports::editor::CursorMovement,
    ui::{
        FastNavigation, KeyStroke, LogicalModifiers, ShortcutContext as Context, UiKey,
        VisualRowEdge,
    },
};

use super::{dispatch::ResolvedShortcut, model::ShortcutActionId as Action};

pub(super) fn resolved(action: Action, intention: UiKey) -> ResolvedShortcut {
    ResolvedShortcut {
        action: Some(action),
        intention,
    }
}

pub(super) fn literal(intention: UiKey) -> ResolvedShortcut {
    ResolvedShortcut {
        action: None,
        intention,
    }
}

pub(super) fn action_intention(action: Action, context: Context, stroke: KeyStroke) -> UiKey {
    if matches!(context, Context::Board | Context::InsertionBoundary)
        && is_board_typed_action(action)
    {
        return UiKey::Shortcut(action);
    }
    if let Some((movement, extend_selection)) = movement_intention(action) {
        return move_key(movement, extend_selection);
    }
    match action {
        Action::Quit => UiKey::Quit,
        Action::Close => UiKey::Escape,
        Action::Confirm => UiKey::Enter,
        Action::Backspace => UiKey::Backspace,
        Action::DeleteForward if stroke.modifiers.is_empty() => UiKey::Delete,
        Action::DeleteForward => UiKey::ModifiedDelete,
        Action::Tab => UiKey::Tab,
        Action::BackTab => UiKey::BackTab,
        Action::SubmitRemove => UiKey::Submit,
        Action::SubmitKeep => UiKey::SubmitKeep,
        Action::Copy
        | Action::Cut
        | Action::PasteExact
        | Action::PasteReflow
        | Action::SelectAll
        | Action::Duplicate
        | Action::Undo
        | Action::Redo
        | Action::DeleteLogicalLine
        | Action::PickerPrevious
        | Action::PickerNext => primary_intention(action),
        Action::DeleteSentence => UiKey::DeleteSentence,
        Action::ContextualTransform => UiKey::Shortcut(Action::ContextualTransform),
        Action::MoveUp => UiKey::PrimaryShiftMove {
            movement: CursorMovement::DocumentStart,
        },
        Action::MoveDown => UiKey::PrimaryShiftMove {
            movement: CursorMovement::DocumentEnd,
        },
        Action::FastPrevious => fast(
            FastNavigation::Previous,
            stroke.modifiers.contains(LogicalModifiers::SHIFT),
        ),
        Action::FastNext => fast(
            FastNavigation::Next,
            stroke.modifiers.contains(LogicalModifiers::SHIFT),
        ),
        Action::ExtendVisualRowStart => UiKey::ExtendVisualRow {
            edge: VisualRowEdge::Start,
        },
        Action::ExtendVisualRowEnd => UiKey::ExtendVisualRow {
            edge: VisualRowEdge::End,
        },
        Action::MoveVisualRowStart => UiKey::MoveVisualRow {
            edge: VisualRowEdge::Start,
        },
        Action::MoveVisualRowEnd => UiKey::MoveVisualRow {
            edge: VisualRowEdge::End,
        },
        _ => UiKey::Shortcut(action),
    }
}

const fn is_board_typed_action(action: Action) -> bool {
    matches!(
        action,
        Action::New
            | Action::Edit
            | Action::Delete
            | Action::FocusPrevious
            | Action::FocusNext
            | Action::ExtendPrevious
            | Action::ExtendNext
            | Action::MoveUp
            | Action::MoveDown
            | Action::Collapse
            | Action::Select
            | Action::RangeSelect
            | Action::OpenSearch
            | Action::OpenCommands
            | Action::Help
            | Action::ScreenshotInbox
            | Action::ContextualTransform
    )
}

const fn movement_intention(action: Action) -> Option<(CursorMovement, bool)> {
    let movement = match action {
        Action::FocusPrevious | Action::MoveVisualUp | Action::ChooseUp => {
            (CursorMovement::VisualUp, false)
        }
        Action::FocusNext | Action::MoveVisualDown | Action::ChooseDown => {
            (CursorMovement::VisualDown, false)
        }
        Action::ExtendPrevious | Action::ExtendVisualUp => (CursorMovement::VisualUp, true),
        Action::ExtendNext | Action::ExtendVisualDown => (CursorMovement::VisualDown, true),
        Action::MoveGraphemeBack | Action::ChooseLeft => (CursorMovement::GraphemeBack, false),
        Action::MoveGraphemeForward | Action::ChooseRight => {
            (CursorMovement::GraphemeForward, false)
        }
        Action::MoveWordBack => (CursorMovement::WordBack, false),
        Action::MoveWordForward => (CursorMovement::WordForward, false),
        Action::MoveDocumentStart => (CursorMovement::DocumentStart, false),
        Action::MoveDocumentEnd => (CursorMovement::DocumentEnd, false),
        Action::MoveLineStart => (CursorMovement::LineStart, false),
        Action::MoveLineEnd => (CursorMovement::LineEnd, false),
        Action::ExtendGraphemeBack => (CursorMovement::GraphemeBack, true),
        Action::ExtendGraphemeForward => (CursorMovement::GraphemeForward, true),
        Action::ExtendWordBack => (CursorMovement::WordBack, true),
        Action::ExtendWordForward => (CursorMovement::WordForward, true),
        Action::ExtendDocumentStart => (CursorMovement::DocumentStart, true),
        Action::ExtendDocumentEnd => (CursorMovement::DocumentEnd, true),
        Action::ExtendLineStart => (CursorMovement::LineStart, true),
        Action::ExtendLineEnd => (CursorMovement::LineEnd, true),
        _ => return None,
    };
    Some(movement)
}

pub(super) fn primary_intention(action: Action) -> UiKey {
    match action {
        Action::Copy => UiKey::Copy,
        Action::Cut => UiKey::Cut,
        Action::PasteExact => UiKey::PasteClipboard,
        Action::PasteReflow => UiKey::PasteClipboardReflow,
        Action::SelectAll => UiKey::SelectAll,
        Action::Duplicate => UiKey::Duplicate,
        Action::Undo => UiKey::Undo,
        Action::Redo => UiKey::Redo,
        Action::DeleteLogicalLine => UiKey::DeleteLogicalLine,
        Action::PickerPrevious => UiKey::PickerPrevious,
        Action::PickerNext => UiKey::PickerNext,
        _ => UiKey::Shortcut(action),
    }
}

pub(super) fn move_key(movement: CursorMovement, extend_selection: bool) -> UiKey {
    UiKey::Move {
        movement,
        extend_selection,
    }
}

pub(super) fn fast(direction: FastNavigation, extend_selection: bool) -> UiKey {
    UiKey::FastNavigation {
        direction,
        extend_selection,
    }
}

pub(super) fn has_command_modifier(modifiers: LogicalModifiers) -> bool {
    modifiers.intersects(
        LogicalModifiers::CONTROL
            .union(LogicalModifiers::SUPER)
            .union(LogicalModifiers::META)
            .union(LogicalModifiers::HYPER),
    )
}

pub(super) fn opposite_ascii_case(character: char) -> Option<char> {
    if character.is_ascii_lowercase() {
        Some(character.to_ascii_uppercase())
    } else if character.is_ascii_uppercase() {
        Some(character.to_ascii_lowercase())
    } else {
        None
    }
}
