//! Pointer-event normalization contracts.

use crossterm::event::{Event, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

use crate::ui::{PointerButton, PointerInput, PointerKind, UiInput};

use super::translate;

#[test]
fn mouse_coordinates_are_normalized_without_terminal_types() {
    let event = Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 7,
        row: 3,
        modifiers: KeyModifiers::NONE,
    });
    assert_eq!(
        translate(event),
        Some(UiInput::Pointer(PointerInput {
            column: 7,
            row: 3,
            kind: PointerKind::Down(PointerButton::Left),
            extend_selection: false,
        }))
    );
}

#[test]
fn shifted_mouse_input_preserves_selection_extension_intent() {
    let event = Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 4,
        row: 2,
        modifiers: KeyModifiers::SHIFT,
    });
    assert_eq!(
        translate(event),
        Some(UiInput::Pointer(PointerInput {
            column: 4,
            row: 2,
            kind: PointerKind::Down(PointerButton::Left),
            extend_selection: true,
        }))
    );
}

#[test]
fn each_vertical_wheel_event_remains_one_directional_pointer_intention() {
    for (mouse, expected) in [
        (MouseEventKind::ScrollUp, PointerKind::ScrollUp),
        (MouseEventKind::ScrollDown, PointerKind::ScrollDown),
    ] {
        let event = Event::Mouse(MouseEvent {
            kind: mouse,
            column: 9,
            row: 4,
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(
            translate(event),
            Some(UiInput::Pointer(PointerInput {
                column: 9,
                row: 4,
                kind: expected,
                extend_selection: false,
            }))
        );
    }
}
