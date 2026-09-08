//! Board modifier ladders and insertion-row ownership boundaries.

use super::navigation::{durable_thought, visual};
use super::*;

fn populated() -> Fixture {
    let mut fixture = Fixture::new();
    for content in ["first", "second", "third"] {
        durable_thought(&mut fixture, content);
    }
    fixture
}

fn focus_content(fixture: &Fixture) -> &str {
    let focused = fixture.app.state.focused_thought.expect("focused thought");
    &fixture
        .app
        .state
        .board
        .thought(focused)
        .expect("thought")
        .content
}

fn assert_focus_up(input: UiInput) {
    let mut fixture = populated();
    fixture.input(input);
    assert_eq!(focus_content(&fixture), "second");
    assert!(super::movement_symmetry::selected(&fixture).is_empty());
}

#[test]
fn unsupported_board_modifiers_keep_the_base_focus_intention() {
    for key in [
        UiInput::KeyStroke(
            KeyStroke::press(LogicalKey::Up).with_modifiers(LogicalModifiers::HYPER),
        ),
        UiInput::KeyStroke(
            KeyStroke::press(LogicalKey::Up).with_modifiers(LogicalModifiers::SUPER),
        ),
        UiInput::KeyStroke(KeyStroke::press(LogicalKey::Up).with_modifiers(LogicalModifiers::META)),
    ] {
        assert_focus_up(key);
    }
}

#[test]
fn shifted_and_primary_shifted_spellings_keep_range_and_reorder() {
    for key in [
        UiKey::Move {
            movement: CursorMovement::VisualUp,
            extend_selection: true,
        },
        UiKey::Character('K'),
    ] {
        let mut fixture = populated();
        fixture.input(crate::key_input(key));
        assert_eq!(
            super::movement_symmetry::selected(&fixture),
            ["second", "third"]
        );
        assert_eq!(
            super::movement_symmetry::order(&fixture),
            ["first", "second", "third"]
        );
    }

    for key in [
        UiKey::PrimaryShiftMove {
            movement: CursorMovement::VisualUp,
        },
        UiKey::PrimaryShiftMove {
            movement: CursorMovement::DocumentStart,
        },
        UiKey::PrimaryShiftCharacter('K'),
        UiKey::PrimaryShiftCharacter('k'),
    ] {
        let mut fixture = populated();
        fixture.input(crate::key_input(key));
        assert!(super::movement_symmetry::selected(&fixture).is_empty());
        assert_eq!(
            super::movement_symmetry::order(&fixture),
            ["first", "third", "second"]
        );
    }
}

#[cfg(target_os = "macos")]
#[test]
fn option_shift_arrows_reorder_on_board_but_not_at_the_insertion_boundary() {
    let option_shift = LogicalModifiers::ALT.union(LogicalModifiers::SHIFT);
    let mut fixture = populated();
    fixture.input(UiInput::KeyStroke(
        KeyStroke::press(LogicalKey::Up).with_modifiers(option_shift),
    ));
    assert_eq!(
        super::movement_symmetry::order(&fixture),
        ["first", "third", "second"]
    );

    let mut insertion = populated();
    insertion.input(crate::key_input(UiKey::Move {
        movement: CursorMovement::VisualDown,
        extend_selection: false,
    }));
    insertion.input(UiInput::KeyStroke(
        KeyStroke::press(LogicalKey::Down).with_modifiers(option_shift),
    ));
    assert!(insertion.app.insertion_focused());
    assert_eq!(
        super::movement_symmetry::order(&insertion),
        ["first", "second", "third"]
    );
}

#[test]
fn insertion_row_rejects_thought_only_range_and_reorder_intentions() {
    let blocked = [
        UiKey::Move {
            movement: CursorMovement::VisualDown,
            extend_selection: true,
        },
        UiKey::Character('J'),
        UiKey::PrimaryShiftMove {
            movement: CursorMovement::DocumentEnd,
        },
        UiKey::PrimaryCharacter('J'),
    ];
    for key in blocked {
        let mut fixture = populated();
        fixture.input(visual(CursorMovement::VisualDown, false));
        assert!(fixture.app.insertion_focused());

        fixture.input(crate::key_input(key));
        fixture.input(visual(CursorMovement::VisualDown, false));
        assert_eq!(fixture.app.state.board.live_thoughts().len(), 3);
        assert!(fixture.app.insertion_focused());

        fixture.input(crate::key_input(UiKey::Character('j')));
        assert_eq!(fixture.app.state.board.live_thoughts().len(), 4);
    }
}

#[test]
fn insertion_boundary_rejects_relative_insert_actions_without_a_live_focus() {
    let mut fixture = populated();
    fixture.input(visual(CursorMovement::VisualDown, false));
    fixture.input(UiInput::KeyStroke(
        KeyStroke::press(LogicalKey::Down).with_modifiers(LogicalModifiers::ALT),
    ));

    assert_eq!(fixture.app.state.board.live_thoughts().len(), 3);
    assert!(fixture.app.insertion_focused());
}

#[test]
fn remapped_vertical_bindings_share_the_same_modifier_ladder() {
    let settings = UiSettings {
        shortcuts: proqi::ui::ShortcutRegistry::from_legacy(&proqi::ui::KeyBindings {
            focus_up: 'i',
            focus_down: 'm',
            range_up: 'I',
            range_down: 'M',
            screenshot_inbox: 'b',
            ..proqi::ui::KeyBindings::default()
        })
        .expect("valid translated keymap"),
        ..UiSettings::default()
    };
    let mut fixture = Fixture::with_settings(settings);
    for content in ["first", "second", "third"] {
        durable_thought(&mut fixture, content);
    }

    fixture.input(crate::key_input(UiKey::PrimaryCharacter('i')));
    assert_eq!(focus_content(&fixture), "second");
    fixture.input(crate::key_input(UiKey::Character('I')));
    assert_eq!(
        super::movement_symmetry::selected(&fixture),
        ["first", "second"]
    );
    fixture.input(crate::key_input(UiKey::Escape));
    fixture.input(crate::key_input(UiKey::PrimaryCharacter('I')));
    assert_eq!(
        super::movement_symmetry::order(&fixture),
        ["second", "third", "first"]
    );
}
