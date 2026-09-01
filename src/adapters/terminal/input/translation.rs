//! Crossterm event translation into terminal-independent UI intentions.

use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};

use crate::{
    ports::editor::CursorMovement,
    ui::{PointerButton, PointerInput, PointerKind, UiInput, UiKey},
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
    let primary = key
        .modifiers
        .intersects(KeyModifiers::CONTROL | KeyModifiers::SUPER | KeyModifiers::META);
    if primary && let Some(command) = primary_key(&key) {
        return Some(command);
    }
    let extend_selection = key.modifiers.contains(KeyModifiers::SHIFT);
    let word = key
        .modifiers
        .intersects(KeyModifiers::ALT | KeyModifiers::CONTROL);
    let document = key
        .modifiers
        .intersects(KeyModifiers::SUPER | KeyModifiers::META);
    match key.code {
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
            document,
            extend_selection,
        )),
        KeyCode::Down => Some(vertical_navigation(
            CursorMovement::VisualDown,
            CursorMovement::VisualJumpDown,
            CursorMovement::DocumentEnd,
            key.modifiers,
            primary,
            document,
            extend_selection,
        )),
        KeyCode::Left => Some(move_key(
            if word {
                CursorMovement::WordBack
            } else {
                CursorMovement::GraphemeBack
            },
            extend_selection,
        )),
        KeyCode::Right => Some(move_key(
            if word {
                CursorMovement::WordForward
            } else {
                CursorMovement::GraphemeForward
            },
            extend_selection,
        )),
        KeyCode::Home => Some(move_key(CursorMovement::LineStart, extend_selection)),
        KeyCode::End => Some(move_key(CursorMovement::LineEnd, extend_selection)),
        _ => None,
    }
}

fn primary_key(key: &KeyEvent) -> Option<UiKey> {
    match key.code {
        KeyCode::Enter if key.modifiers.contains(KeyModifiers::SHIFT) => Some(UiKey::SubmitKeep),
        KeyCode::Enter => Some(UiKey::Submit),
        KeyCode::Char('a') => Some(UiKey::SelectAll),
        KeyCode::Char('c') => Some(UiKey::Copy),
        KeyCode::Char('x') => Some(UiKey::Cut),
        KeyCode::Char('v') => Some(UiKey::PasteClipboard),
        KeyCode::Char('d') => Some(UiKey::Duplicate),
        KeyCode::Char('q') => Some(UiKey::Quit),
        KeyCode::Char('u') => Some(UiKey::DeleteLine),
        KeyCode::Char('z') if key.modifiers.contains(KeyModifiers::SHIFT) => Some(UiKey::Redo),
        KeyCode::Char('z') => Some(UiKey::Undo),
        KeyCode::Char('y') => Some(UiKey::Redo),
        KeyCode::Char('p') => Some(UiKey::PickerPrevious),
        KeyCode::Char('n') => Some(UiKey::PickerNext),
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
    legacy_document: bool,
    extend_selection: bool,
) -> UiKey {
    let legacy = if legacy_document { boundary } else { ordinary };
    if extend_selection {
        return vertical_key(legacy, primary, true);
    }
    let platform_primary = if cfg!(target_os = "macos") {
        modifiers.intersects(KeyModifiers::SUPER | KeyModifiers::META)
    } else {
        modifiers.contains(KeyModifiers::CONTROL)
    };
    let editor_movement = if platform_primary {
        boundary
    } else if modifiers.contains(KeyModifiers::ALT) {
        accelerated
    } else {
        return vertical_key(legacy, primary, false);
    };
    UiKey::EditNavigation {
        editor_movement,
        board_movement: legacy,
    }
}

const fn vertical_key(movement: CursorMovement, primary: bool, extend_selection: bool) -> UiKey {
    if primary && extend_selection {
        UiKey::PrimaryShiftMove { movement }
    } else {
        move_key(movement, extend_selection)
    }
}

const fn move_key(movement: CursorMovement, extend_selection: bool) -> UiKey {
    UiKey::Move {
        movement,
        extend_selection,
    }
}
