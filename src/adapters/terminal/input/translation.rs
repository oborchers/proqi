//! Crossterm event translation into terminal-independent UI intentions.

use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};

use crate::{
    ports::editor::CursorMovement,
    ui::{FastNavigation, PointerButton, PointerInput, PointerKind, UiInput, UiKey, VisualRowEdge},
};

pub(super) fn translate(event: Event) -> Option<UiInput> {
    match event {
        Event::Key(key) if matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) => {
            translate_key(key).map(UiInput::Key)
        }
        Event::Paste(content) => Some(
            super::super::path_import::annotate_existing_files(&content)
                .map_or_else(|| UiInput::Paste(content), UiInput::PasteAnnotated),
        ),
        Event::Resize(width, height) => Some(UiInput::Resize { width, height }),
        Event::Mouse(mouse) => translate_mouse(mouse).map(UiInput::Pointer),
        Event::FocusGained => Some(UiInput::HostFocusGained),
        Event::FocusLost => Some(UiInput::HostFocusLost),
        Event::Key(_) => None,
    }
}

fn translate_mouse(mouse: MouseEvent) -> Option<PointerInput> {
    let kind = match mouse.kind {
        MouseEventKind::Down(button) => PointerKind::Down(pointer_button(button)),
        MouseEventKind::Up(button) => PointerKind::Up(pointer_button(button)),
        MouseEventKind::Drag(button) => PointerKind::Drag(pointer_button(button)),
        MouseEventKind::Moved => PointerKind::Move,
        MouseEventKind::ScrollUp => PointerKind::ScrollUp,
        MouseEventKind::ScrollDown => PointerKind::ScrollDown,
        MouseEventKind::ScrollLeft | MouseEventKind::ScrollRight => return None,
    };
    Some(PointerInput {
        column: mouse.column,
        row: mouse.row,
        kind,
        extend_selection: mouse.modifiers.contains(KeyModifiers::SHIFT),
    })
}

const fn pointer_button(button: MouseButton) -> PointerButton {
    match button {
        MouseButton::Left => PointerButton::Left,
        MouseButton::Middle => PointerButton::Middle,
        MouseButton::Right => PointerButton::Right,
    }
}

pub(super) fn translate_key(key: KeyEvent) -> Option<UiKey> {
    translate_key_for_platform(key, ModifierPlatform::current())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ModifierPlatform {
    MacOs,
    Other,
}

impl ModifierPlatform {
    const fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::MacOs
        } else {
            Self::Other
        }
    }
}

pub(super) fn translate_key_for_platform(
    key: KeyEvent,
    platform: ModifierPlatform,
) -> Option<UiKey> {
    let primary = platform_primary(key.modifiers, platform);
    if primary && let Some(command) = primary_key(&key) {
        return Some(command);
    }
    if has_command_modifier(key.modifiers) && primary_key(&key).is_some() {
        return None;
    }
    let extend_selection = key.modifiers.contains(KeyModifiers::SHIFT);
    if let Some(horizontal) = horizontal_key(&key, platform, extend_selection) {
        return Some(horizontal);
    }
    match key.code {
        KeyCode::Char(' ') if key.modifiers.is_empty() => Some(UiKey::UnmodifiedSpace),
        KeyCode::Char(character) => Some(UiKey::Character(character)),
        KeyCode::Enter => Some(UiKey::Enter),
        KeyCode::BackTab => Some(UiKey::BackTab),
        KeyCode::Tab if extend_selection => Some(UiKey::BackTab),
        KeyCode::Tab => Some(UiKey::Tab),
        KeyCode::Esc => Some(UiKey::Escape),
        KeyCode::Backspace => Some(UiKey::Backspace),
        KeyCode::Delete if key.modifiers.is_empty() => Some(UiKey::Delete),
        KeyCode::Delete => Some(UiKey::ModifiedDelete),
        KeyCode::Up => Some(vertical_navigation(
            CursorMovement::VisualUp,
            CursorMovement::VisualJumpUp,
            CursorMovement::DocumentStart,
            key.modifiers,
            primary,
            extend_selection,
        )),
        KeyCode::Down => Some(vertical_navigation(
            CursorMovement::VisualDown,
            CursorMovement::VisualJumpDown,
            CursorMovement::DocumentEnd,
            key.modifiers,
            primary,
            extend_selection,
        )),
        KeyCode::PageUp => Some(fast_navigation(FastNavigation::Previous, extend_selection)),
        KeyCode::PageDown => Some(fast_navigation(FastNavigation::Next, extend_selection)),
        KeyCode::Home => Some(move_key(CursorMovement::LineStart, extend_selection)),
        KeyCode::End => Some(move_key(CursorMovement::LineEnd, extend_selection)),
        _ => None,
    }
}

fn horizontal_key(
    key: &KeyEvent,
    platform: ModifierPlatform,
    extend_selection: bool,
) -> Option<UiKey> {
    let edge = match key.code {
        KeyCode::Left => VisualRowEdge::Start,
        KeyCode::Right => VisualRowEdge::End,
        _ => return None,
    };
    if platform == ModifierPlatform::MacOs && platform_primary(key.modifiers, platform) {
        return Some(if extend_selection {
            UiKey::ExtendVisualRow { edge }
        } else {
            UiKey::MoveVisualRow { edge }
        });
    }
    let word = match platform {
        ModifierPlatform::MacOs => key.modifiers.contains(KeyModifiers::ALT),
        ModifierPlatform::Other => key.modifiers.contains(KeyModifiers::CONTROL),
    };
    let movement = match (edge, word) {
        (VisualRowEdge::Start, true) => CursorMovement::WordBack,
        (VisualRowEdge::Start, false) => CursorMovement::GraphemeBack,
        (VisualRowEdge::End, true) => CursorMovement::WordForward,
        (VisualRowEdge::End, false) => CursorMovement::GraphemeForward,
    };
    Some(move_key(movement, extend_selection))
}

fn primary_key(key: &KeyEvent) -> Option<UiKey> {
    let shifted = key.modifiers.contains(KeyModifiers::SHIFT);
    match key.code {
        KeyCode::Enter if shifted => Some(UiKey::SubmitKeep),
        KeyCode::Enter => Some(UiKey::Submit),
        KeyCode::Char('v' | 'V') if shifted => Some(UiKey::PasteClipboardReflow),
        KeyCode::Char(character @ ('a' | 'A' | 'c' | 'C' | 'x' | 'X' | 'd' | 'D' | 'q' | 'Q'))
            if shifted =>
        {
            Some(UiKey::PrimaryShiftCharacter(character))
        }
        KeyCode::Char('a' | 'A') => Some(UiKey::SelectAll),
        KeyCode::Char('c' | 'C') => Some(UiKey::Copy),
        KeyCode::Char('x' | 'X') => Some(UiKey::Cut),
        KeyCode::Char('v' | 'V') => Some(UiKey::PasteClipboard),
        KeyCode::Char('d' | 'D') => Some(UiKey::Duplicate),
        KeyCode::Char('q' | 'Q') => Some(UiKey::Quit),
        KeyCode::Char('u' | 'U') if !shifted => Some(UiKey::DeleteLogicalLine),
        KeyCode::Char('z' | 'Z') if shifted => Some(UiKey::Redo),
        KeyCode::Char(character @ ('y' | 'Y')) if shifted => {
            Some(UiKey::PrimaryShiftCharacter(character))
        }
        KeyCode::Char('y' | 'Y') => Some(UiKey::Redo),
        KeyCode::Char('z' | 'Z') => Some(UiKey::Undo),
        KeyCode::Char('p' | 'P') => Some(UiKey::PickerPrevious),
        KeyCode::Char('n' | 'N') => Some(UiKey::PickerNext),
        KeyCode::Char(character) if shifted || character.is_uppercase() => {
            Some(UiKey::PrimaryShiftCharacter(character))
        }
        KeyCode::Char(character) => Some(UiKey::PrimaryCharacter(character)),
        _ => None,
    }
}

fn vertical_navigation(
    ordinary: CursorMovement,
    accelerated: CursorMovement,
    boundary: CursorMovement,
    modifiers: KeyModifiers,
    primary: bool,
    extend_selection: bool,
) -> UiKey {
    if modifiers.contains(KeyModifiers::ALT) && !primary {
        return fast_navigation(
            if accelerated == CursorMovement::VisualJumpUp {
                FastNavigation::Previous
            } else {
                FastNavigation::Next
            },
            extend_selection,
        );
    }
    if primary && extend_selection {
        return UiKey::PrimaryShiftMove { movement: boundary };
    }
    if extend_selection {
        return move_key(ordinary, true);
    }
    UiKey::EditNavigation {
        editor_movement: if primary { boundary } else { ordinary },
        board_movement: ordinary,
    }
}

fn platform_primary(modifiers: KeyModifiers, platform: ModifierPlatform) -> bool {
    let primary = match platform {
        ModifierPlatform::MacOs => match modifiers & (KeyModifiers::SUPER | KeyModifiers::META) {
            KeyModifiers::SUPER => Some(KeyModifiers::SUPER),
            KeyModifiers::META => Some(KeyModifiers::META),
            _ => None,
        },
        ModifierPlatform::Other => modifiers
            .contains(KeyModifiers::CONTROL)
            .then_some(KeyModifiers::CONTROL),
    };
    primary.is_some_and(|primary| {
        modifiers
            .difference(primary | KeyModifiers::SHIFT)
            .is_empty()
    })
}

fn has_command_modifier(modifiers: KeyModifiers) -> bool {
    modifiers.intersects(
        KeyModifiers::CONTROL | KeyModifiers::SUPER | KeyModifiers::META | KeyModifiers::HYPER,
    )
}

const fn fast_navigation(direction: FastNavigation, extend_selection: bool) -> UiKey {
    UiKey::FastNavigation {
        direction,
        extend_selection,
    }
}

const fn move_key(movement: CursorMovement, extend_selection: bool) -> UiKey {
    UiKey::Move {
        movement,
        extend_selection,
    }
}
