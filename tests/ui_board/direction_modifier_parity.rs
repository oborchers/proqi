//! Four-way non-text chooser parity across irrelevant modifiers.

use super::*;
use proqi::domain::Direction;

#[test]
fn direction_chooser_accepts_modified_arrows_and_vim_spellings_equally() {
    let cases = [
        (UiKey::Character('H'), Direction::Left),
        (UiKey::PrimaryCharacter('h'), Direction::Left),
        (
            UiKey::Move {
                movement: CursorMovement::WordBack,
                extend_selection: true,
            },
            Direction::Left,
        ),
        (UiKey::Character('J'), Direction::Down),
        (UiKey::PrimaryCharacter('j'), Direction::Down),
        (
            UiKey::PrimaryShiftMove {
                movement: CursorMovement::DocumentEnd,
            },
            Direction::Down,
        ),
        (UiKey::Character('K'), Direction::Up),
        (UiKey::PrimaryCharacter('k'), Direction::Up),
        (
            UiKey::EditNavigation {
                editor_movement: CursorMovement::VisualJumpUp,
                board_movement: CursorMovement::VisualUp,
            },
            Direction::Up,
        ),
        (UiKey::Character('L'), Direction::Right),
        (UiKey::PrimaryCharacter('l'), Direction::Right),
        (
            UiKey::Move {
                movement: CursorMovement::WordForward,
                extend_selection: true,
            },
            Direction::Right,
        ),
    ];

    for (key, expected) in cases {
        let mut fixture = Fixture::new();
        super::agent::prepare_thought(&mut fixture);
        fixture.app.complete_agent_discovery(Ok(vec![
            super::agent::target(Direction::Left, "w1:p2"),
            super::agent::target(Direction::Down, "w1:p3"),
            super::agent::target(Direction::Up, "w1:p4"),
            super::agent::target(Direction::Right, "w1:p5"),
        ]));
        fixture.input(UiInput::Key(UiKey::Character('S')));
        let effects = fixture.effects(UiInput::Key(key));
        let request = super::agent::start_submission(&mut fixture, &effects);
        assert_eq!(
            request.target.adjacent_direction(),
            Some(expected),
            "key: {key:?}"
        );
    }
}
