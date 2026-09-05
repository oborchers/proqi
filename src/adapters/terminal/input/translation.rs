//! Crossterm event decoding into terminal-independent input values.

use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers, MediaKeyCode,
    ModifierKeyCode, MouseButton, MouseEvent, MouseEventKind,
};

use crate::ui::{
    KeyPhase, KeyStroke, LogicalKey, LogicalKeyState, LogicalMediaKey, LogicalModifierKey,
    LogicalModifiers, PointerButton, PointerInput, PointerKind, UiInput,
};

pub(super) fn translate(event: Event) -> Option<UiInput> {
    match event {
        Event::Key(key) if matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) => {
            Some(UiInput::KeyStroke(decode_key(key)))
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

pub(super) fn decode_key(key: KeyEvent) -> KeyStroke {
    KeyStroke {
        key: decode_code(key.code),
        modifiers: decode_modifiers(key.modifiers),
        phase: decode_phase(key.kind),
        state: decode_state(key.state),
    }
}

const fn decode_phase(kind: KeyEventKind) -> KeyPhase {
    match kind {
        KeyEventKind::Press => KeyPhase::Press,
        KeyEventKind::Repeat => KeyPhase::Repeat,
        KeyEventKind::Release => KeyPhase::Release,
    }
}

fn decode_modifiers(modifiers: KeyModifiers) -> LogicalModifiers {
    let mut decoded = LogicalModifiers::NONE;
    for (source, target) in [
        (KeyModifiers::SHIFT, LogicalModifiers::SHIFT),
        (KeyModifiers::CONTROL, LogicalModifiers::CONTROL),
        (KeyModifiers::ALT, LogicalModifiers::ALT),
        (KeyModifiers::SUPER, LogicalModifiers::SUPER),
        (KeyModifiers::META, LogicalModifiers::META),
        (KeyModifiers::HYPER, LogicalModifiers::HYPER),
    ] {
        if modifiers.contains(source) {
            decoded = decoded.union(target);
        }
    }
    decoded
}

fn decode_state(state: KeyEventState) -> LogicalKeyState {
    let mut decoded = LogicalKeyState::NONE;
    for (source, target) in [
        (KeyEventState::KEYPAD, LogicalKeyState::KEYPAD),
        (KeyEventState::CAPS_LOCK, LogicalKeyState::CAPS_LOCK),
        (KeyEventState::NUM_LOCK, LogicalKeyState::NUM_LOCK),
    ] {
        if state.contains(source) {
            decoded = decoded.union(target);
        }
    }
    decoded
}

const fn decode_code(code: KeyCode) -> LogicalKey {
    match code {
        KeyCode::Backspace => LogicalKey::Backspace,
        KeyCode::Enter => LogicalKey::Enter,
        KeyCode::Left => LogicalKey::Left,
        KeyCode::Right => LogicalKey::Right,
        KeyCode::Up => LogicalKey::Up,
        KeyCode::Down => LogicalKey::Down,
        KeyCode::Home => LogicalKey::Home,
        KeyCode::End => LogicalKey::End,
        KeyCode::PageUp => LogicalKey::PageUp,
        KeyCode::PageDown => LogicalKey::PageDown,
        KeyCode::Tab => LogicalKey::Tab,
        KeyCode::BackTab => LogicalKey::BackTab,
        KeyCode::Delete => LogicalKey::Delete,
        KeyCode::Insert => LogicalKey::Insert,
        KeyCode::F(number) => LogicalKey::Function(number),
        KeyCode::Char(character) => LogicalKey::Character(character),
        KeyCode::Null => LogicalKey::Null,
        KeyCode::Esc => LogicalKey::Escape,
        KeyCode::CapsLock => LogicalKey::CapsLock,
        KeyCode::ScrollLock => LogicalKey::ScrollLock,
        KeyCode::NumLock => LogicalKey::NumLock,
        KeyCode::PrintScreen => LogicalKey::PrintScreen,
        KeyCode::Pause => LogicalKey::Pause,
        KeyCode::Menu => LogicalKey::Menu,
        KeyCode::KeypadBegin => LogicalKey::KeypadBegin,
        KeyCode::Media(key) => LogicalKey::Media(decode_media_key(key)),
        KeyCode::Modifier(key) => LogicalKey::Modifier(decode_modifier_key(key)),
    }
}

const fn decode_media_key(key: MediaKeyCode) -> LogicalMediaKey {
    match key {
        MediaKeyCode::Play => LogicalMediaKey::Play,
        MediaKeyCode::Pause => LogicalMediaKey::Pause,
        MediaKeyCode::PlayPause => LogicalMediaKey::PlayPause,
        MediaKeyCode::Reverse => LogicalMediaKey::Reverse,
        MediaKeyCode::Stop => LogicalMediaKey::Stop,
        MediaKeyCode::FastForward => LogicalMediaKey::FastForward,
        MediaKeyCode::Rewind => LogicalMediaKey::Rewind,
        MediaKeyCode::TrackNext => LogicalMediaKey::TrackNext,
        MediaKeyCode::TrackPrevious => LogicalMediaKey::TrackPrevious,
        MediaKeyCode::Record => LogicalMediaKey::Record,
        MediaKeyCode::LowerVolume => LogicalMediaKey::LowerVolume,
        MediaKeyCode::RaiseVolume => LogicalMediaKey::RaiseVolume,
        MediaKeyCode::MuteVolume => LogicalMediaKey::MuteVolume,
    }
}

const fn decode_modifier_key(key: ModifierKeyCode) -> LogicalModifierKey {
    match key {
        ModifierKeyCode::LeftShift => LogicalModifierKey::LeftShift,
        ModifierKeyCode::LeftControl => LogicalModifierKey::LeftControl,
        ModifierKeyCode::LeftAlt => LogicalModifierKey::LeftAlt,
        ModifierKeyCode::LeftSuper => LogicalModifierKey::LeftSuper,
        ModifierKeyCode::LeftHyper => LogicalModifierKey::LeftHyper,
        ModifierKeyCode::LeftMeta => LogicalModifierKey::LeftMeta,
        ModifierKeyCode::RightShift => LogicalModifierKey::RightShift,
        ModifierKeyCode::RightControl => LogicalModifierKey::RightControl,
        ModifierKeyCode::RightAlt => LogicalModifierKey::RightAlt,
        ModifierKeyCode::RightSuper => LogicalModifierKey::RightSuper,
        ModifierKeyCode::RightHyper => LogicalModifierKey::RightHyper,
        ModifierKeyCode::RightMeta => LogicalModifierKey::RightMeta,
        ModifierKeyCode::IsoLevel3Shift => LogicalModifierKey::IsoLevel3Shift,
        ModifierKeyCode::IsoLevel5Shift => LogicalModifierKey::IsoLevel5Shift,
    }
}
