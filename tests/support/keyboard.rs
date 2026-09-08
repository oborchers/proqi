//! Logical keyboard fixtures that exercise the public registry dispatch path.

use proqi::{
    ports::editor::CursorMovement,
    ui::{FastNavigation, KeyStroke, LogicalKey, LogicalModifiers, UiInput, UiKey, VisualRowEdge},
};

pub(crate) fn key_input(key: UiKey) -> UiInput {
    let primary = if cfg!(target_os = "macos") {
        LogicalModifiers::SUPER
    } else {
        LogicalModifiers::CONTROL
    };
    let (logical_key, modifiers) = match key {
        UiKey::Shortcut(_) => panic!("tests must name the logical stroke, not a resolved action"),
        UiKey::Quit => (LogicalKey::Character('q'), primary),
        UiKey::Character(character) => (LogicalKey::Character(character), LogicalModifiers::NONE),
        UiKey::UnmodifiedSpace => (LogicalKey::Character(' '), LogicalModifiers::NONE),
        UiKey::PrimaryCharacter(character) => (LogicalKey::Character(character), primary),
        UiKey::PrimaryShiftCharacter(character) => (
            LogicalKey::Character(character),
            primary.union(LogicalModifiers::SHIFT),
        ),
        UiKey::Enter => (LogicalKey::Enter, LogicalModifiers::NONE),
        UiKey::Submit => (LogicalKey::Enter, primary),
        UiKey::SubmitKeep => (LogicalKey::Enter, primary.union(LogicalModifiers::SHIFT)),
        UiKey::Tab => (LogicalKey::Tab, LogicalModifiers::NONE),
        UiKey::BackTab => (LogicalKey::BackTab, LogicalModifiers::NONE),
        UiKey::PickerPrevious => (LogicalKey::Character('p'), primary),
        UiKey::PickerNext => (LogicalKey::Character('n'), primary),
        UiKey::FastNavigation {
            direction,
            extend_selection,
        } => (
            match direction {
                FastNavigation::Previous => LogicalKey::PageUp,
                FastNavigation::Next => LogicalKey::PageDown,
            },
            shift_if(extend_selection),
        ),
        UiKey::Escape => (LogicalKey::Escape, LogicalModifiers::NONE),
        UiKey::Backspace => (LogicalKey::Backspace, LogicalModifiers::NONE),
        UiKey::Delete => (LogicalKey::Delete, LogicalModifiers::NONE),
        UiKey::ModifiedDelete => (LogicalKey::Delete, LogicalModifiers::SHIFT),
        UiKey::Move {
            movement,
            extend_selection,
        } => movement_stroke(movement, extend_selection),
        UiKey::ExtendVisualRow { edge } => (
            LogicalKey::Character(match edge {
                VisualRowEdge::Start => 'h',
                VisualRowEdge::End => 'l',
            }),
            primary.union(LogicalModifiers::SHIFT),
        ),
        UiKey::MoveVisualRow { edge } => (
            match edge {
                VisualRowEdge::Start => LogicalKey::Left,
                VisualRowEdge::End => LogicalKey::Right,
            },
            primary,
        ),
        UiKey::PrimaryShiftMove { movement } => {
            let (key, _) = movement_stroke(movement, false);
            (key, primary.union(LogicalModifiers::SHIFT))
        }
        UiKey::SelectAll => (LogicalKey::Character('a'), primary),
        UiKey::DeleteLogicalLine => (LogicalKey::Character('u'), primary),
        UiKey::DeleteSentence => (
            LogicalKey::Character('u'),
            primary.union(LogicalModifiers::SHIFT),
        ),
        UiKey::Undo => (LogicalKey::Character('z'), primary),
        UiKey::Redo => (
            LogicalKey::Character('z'),
            primary.union(LogicalModifiers::SHIFT),
        ),
        UiKey::Copy => (LogicalKey::Character('c'), primary),
        UiKey::Cut => (LogicalKey::Character('x'), primary),
        UiKey::PasteClipboard => (LogicalKey::Character('v'), primary),
        UiKey::PasteClipboardReflow => (
            LogicalKey::Character('v'),
            primary.union(LogicalModifiers::SHIFT),
        ),
        UiKey::Duplicate => (LogicalKey::Character('d'), primary),
    };
    UiInput::KeyStroke(KeyStroke::press(logical_key).with_modifiers(modifiers))
}

fn movement_stroke(
    movement: CursorMovement,
    extend_selection: bool,
) -> (LogicalKey, LogicalModifiers) {
    let (key, base_modifiers) = match movement {
        CursorMovement::GraphemeBack => (LogicalKey::Left, LogicalModifiers::NONE),
        CursorMovement::GraphemeForward => (LogicalKey::Right, LogicalModifiers::NONE),
        CursorMovement::WordBack => (LogicalKey::Left, word_modifier()),
        CursorMovement::WordForward => (LogicalKey::Right, word_modifier()),
        CursorMovement::VisualUp => (LogicalKey::Up, LogicalModifiers::NONE),
        CursorMovement::VisualDown => (LogicalKey::Down, LogicalModifiers::NONE),
        CursorMovement::VisualJumpUp => (LogicalKey::PageUp, LogicalModifiers::NONE),
        CursorMovement::VisualJumpDown => (LogicalKey::PageDown, LogicalModifiers::NONE),
        CursorMovement::DocumentStart => (LogicalKey::Up, LogicalModifiers::CONTROL),
        CursorMovement::DocumentEnd => (LogicalKey::Down, LogicalModifiers::CONTROL),
        CursorMovement::LineStart => (LogicalKey::Left, line_modifier()),
        CursorMovement::LineEnd => (LogicalKey::Right, line_modifier()),
    };
    (key, base_modifiers.union(shift_if(extend_selection)))
}

const fn shift_if(enabled: bool) -> LogicalModifiers {
    if enabled {
        LogicalModifiers::SHIFT
    } else {
        LogicalModifiers::NONE
    }
}

const fn word_modifier() -> LogicalModifiers {
    if cfg!(target_os = "macos") {
        LogicalModifiers::ALT
    } else {
        LogicalModifiers::CONTROL
    }
}

const fn line_modifier() -> LogicalModifiers {
    if cfg!(target_os = "macos") {
        LogicalModifiers::CONTROL
    } else {
        LogicalModifiers::ALT
    }
}
