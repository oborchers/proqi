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

fn assert_focus_up(key: UiKey) {
    let mut fixture = populated();
    fixture.input(UiInput::Key(key));
    assert_eq!(focus_content(&fixture), "second");
    assert!(super::movement_symmetry::selected(&fixture).is_empty());
}

#[test]
fn unsupported_board_modifiers_keep_the_base_focus_intention() {
    for key in [
        UiKey::PrimaryCharacter('k'),
        UiKey::EditNavigation {
            editor_movement: CursorMovement::VisualJumpUp,
            board_movement: CursorMovement::VisualUp,
        },
        UiKey::Move {
            movement: CursorMovement::VisualJumpUp,
            extend_selection: false,
        },
        UiKey::Move {
            movement: CursorMovement::DocumentStart,
            extend_selection: false,
        },
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
        UiKey::Move {
            movement: CursorMovement::VisualJumpUp,
            extend_selection: true,
        },
        UiKey::Character('K'),
    ] {
        let mut fixture = populated();
        fixture.input(UiInput::Key(key));
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
        UiKey::PrimaryCharacter('K'),
    ] {
        let mut fixture = populated();
        fixture.input(UiInput::Key(key));
        assert!(super::movement_symmetry::selected(&fixture).is_empty());
        assert_eq!(
            super::movement_symmetry::order(&fixture),
            ["first", "third", "second"]
        );
    }
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

        fixture.input(UiInput::Key(key));
        fixture.input(visual(CursorMovement::VisualDown, false));
        assert_eq!(fixture.app.state.board.live_thoughts().len(), 3);
        assert!(fixture.app.insertion_focused());

        fixture.input(UiInput::Key(UiKey::Character('j')));
        assert_eq!(fixture.app.state.board.live_thoughts().len(), 4);
    }
}

#[test]
fn insertion_boundary_accepts_mixed_unsupported_focus_modifiers() {
    let mut fixture = populated();
    fixture.input(visual(CursorMovement::VisualDown, false));
    fixture.input(UiInput::Key(UiKey::PrimaryCharacter('j')));
    fixture.input(UiInput::Key(UiKey::EditNavigation {
        editor_movement: CursorMovement::VisualJumpDown,
        board_movement: CursorMovement::VisualDown,
    }));

    assert_eq!(fixture.app.state.board.live_thoughts().len(), 4);
    assert!(matches!(
        fixture.app.interaction_mode(),
        proqi::application::InteractionMode::Edit { .. }
    ));
}

#[test]
fn remapped_vertical_bindings_share_the_same_modifier_ladder() {
    let mut settings = UiSettings::default();
    settings.keybindings.focus_up = 'i';
    settings.keybindings.focus_down = 'm';
    settings.keybindings.range_up = 'I';
    settings.keybindings.range_down = 'M';
    let mut fixture = Fixture::with_settings(settings);
    for content in ["first", "second", "third"] {
        durable_thought(&mut fixture, content);
    }

    fixture.input(UiInput::Key(UiKey::PrimaryCharacter('i')));
    assert_eq!(focus_content(&fixture), "second");
    fixture.input(UiInput::Key(UiKey::Character('I')));
    assert_eq!(
        super::movement_symmetry::selected(&fixture),
        ["first", "second"]
    );
    fixture.input(UiInput::Key(UiKey::Escape));
    fixture.input(UiInput::Key(UiKey::PrimaryCharacter('I')));
    assert_eq!(
        super::movement_symmetry::order(&fixture),
        ["second", "third", "first"]
    );
}
