use super::navigation::{durable_thought, visual};
use super::*;

use proqi::application::InteractionMode;

#[test]
fn compose_cursor_movement_never_creates_the_first_thought() {
    let mut fixture = Fixture::new();

    assert!(
        fixture
            .effects(super::navigation::visual(CursorMovement::VisualDown, false))
            .is_empty()
    );
    assert!(fixture.app.state.board.live_thoughts().is_empty());
    let effects = fixture.effects(super::navigation::visual(CursorMovement::VisualDown, false));

    assert!(effects.is_empty());
    assert!(fixture.app.state.board.live_thoughts().is_empty());
    assert_eq!(fixture.app.interaction_mode(), InteractionMode::Compose);
    assert_eq!(
        fixture.app.editor_snapshot().expect("blank editor").content,
        ""
    );
}

#[test]
fn configured_next_and_arrow_down_share_the_insertion_confirmation() {
    let mut settings = UiSettings::default();
    settings.keybindings.focus_down = 'g';
    let mut fixture = Fixture::with_settings(settings);
    super::navigation::durable_thought(&mut fixture, "existing");
    fixture.input(super::navigation::visual(CursorMovement::VisualDown, false));
    assert!(fixture.app.insertion_focused());

    fixture.input(UiInput::Key(UiKey::Character('g')));
    assert_eq!(fixture.app.state.board.live_thoughts().len(), 1);
    fixture.input(super::navigation::visual(CursorMovement::VisualDown, false));

    assert_eq!(fixture.app.state.board.live_thoughts().len(), 2);
    assert!(matches!(
        fixture.app.interaction_mode(),
        InteractionMode::Edit { .. }
    ));
}

#[test]
fn unrelated_input_resets_insertion_confirmation() {
    let mut fixture = Fixture::new();
    fixture.input(UiInput::Key(UiKey::Escape));
    fixture.input(super::navigation::visual(CursorMovement::VisualDown, false));
    fixture.input(UiInput::Key(UiKey::Character('?')));
    fixture.input(UiInput::Key(UiKey::Escape));

    fixture.input(super::navigation::visual(CursorMovement::VisualDown, false));
    assert!(fixture.app.state.board.live_thoughts().is_empty());
    fixture.input(super::navigation::visual(CursorMovement::VisualDown, false));
    assert!(fixture.app.state.board.live_thoughts().is_empty());
    assert_eq!(fixture.app.interaction_mode(), InteractionMode::Compose);
}

#[test]
fn shifted_down_does_not_arm_insertion_creation() {
    let mut fixture = Fixture::new();
    fixture.input(UiInput::Key(UiKey::Escape));
    fixture.input(super::navigation::visual(CursorMovement::VisualDown, true));
    fixture.input(super::navigation::visual(CursorMovement::VisualDown, false));

    assert!(fixture.app.state.board.live_thoughts().is_empty());
}

#[test]
fn board_top_accepts_up_previous_and_mixed_semantic_spellings() {
    for first in [
        visual(CursorMovement::VisualUp, false),
        UiInput::Key(UiKey::Character('k')),
    ] {
        for second in [
            visual(CursorMovement::VisualUp, false),
            UiInput::Key(UiKey::Character('k')),
        ] {
            let mut fixture = Fixture::new();
            durable_thought(&mut fixture, "former first");
            let former = fixture.app.state.board.live_thoughts()[0].id;

            assert!(fixture.effects(first.clone()).is_empty());
            let effects = fixture.effects(second.clone());
            let live = fixture.app.state.board.live_thoughts();

            assert_eq!(effects.len(), 1);
            assert_eq!(live.len(), 2);
            assert_eq!(live[0].content, "");
            assert_eq!(live[1].id, former);
            assert!(matches!(
                fixture.app.interaction_mode(),
                InteractionMode::Edit { thought_id } if thought_id == live[0].id
            ));
        }
    }
}

#[test]
fn board_top_confirmation_requires_two_consecutive_plain_intentions() {
    let mut fixture = Fixture::new();
    durable_thought(&mut fixture, "first");
    durable_thought(&mut fixture, "middle");
    durable_thought(&mut fixture, "last");

    fixture.input(visual(CursorMovement::VisualUp, false));
    fixture.input(visual(CursorMovement::VisualUp, false));
    assert_eq!(fixture.app.state.board.live_thoughts().len(), 3);
    fixture.input(UiInput::Key(UiKey::Character('?')));
    fixture.input(UiInput::Key(UiKey::Escape));
    fixture.input(visual(CursorMovement::VisualUp, false));
    assert_eq!(fixture.app.state.board.live_thoughts().len(), 3);
    fixture.input(visual(CursorMovement::VisualUp, true));
    fixture.input(visual(CursorMovement::VisualUp, false));
    assert_eq!(fixture.app.state.board.live_thoughts().len(), 3);
    fixture.input(visual(CursorMovement::VisualUp, false));
    assert_eq!(fixture.app.state.board.live_thoughts().len(), 4);
}

#[test]
fn top_creation_clears_arbitrary_selection_and_round_trips_board_history() {
    let mut fixture = Fixture::new();
    durable_thought(&mut fixture, "first");
    durable_thought(&mut fixture, "second");
    fixture.input(UiInput::Key(UiKey::Character('k')));
    fixture.input(UiInput::Key(UiKey::Character(' ')));
    let selected = fixture
        .app
        .state
        .focused_thought
        .expect("focused selection");
    assert!(fixture.app.thought_selected(selected));
    let original = fixture
        .app
        .state
        .board
        .live_thoughts()
        .iter()
        .map(|thought| thought.id)
        .collect::<Vec<_>>();

    fixture.input(UiInput::Key(UiKey::Character('k')));
    fixture.input(visual(CursorMovement::VisualUp, false));
    assert!(!fixture.app.thought_selected(selected));
    let blank = fixture.app.state.board.live_thoughts()[0].id;
    fixture.input(UiInput::Key(UiKey::Escape));
    fixture.input(UiInput::Key(UiKey::Undo));
    assert_eq!(
        fixture
            .app
            .state
            .board
            .live_thoughts()
            .iter()
            .map(|thought| thought.id)
            .collect::<Vec<_>>(),
        original
    );
    fixture.input(UiInput::Key(UiKey::Redo));
    assert_eq!(fixture.app.state.board.live_thoughts()[0].id, blank);
}

#[test]
fn edit_top_creates_before_the_source_only_after_blocked_up() {
    let mut fixture = Fixture::new();
    durable_thought(
        &mut fixture,
        "Grüße 👩‍💻\tcontrol\u{7} and enough text to wrap across several visual rows",
    );
    let source = fixture.app.state.board.live_thoughts()[0].id;
    fixture.input(UiInput::Key(UiKey::Enter));
    let _terminal = draw(&mut fixture, 24, 8);
    fixture.input(UiInput::Key(UiKey::Move {
        movement: CursorMovement::DocumentEnd,
        extend_selection: false,
    }));
    fixture.input(visual(CursorMovement::VisualUp, false));
    assert_eq!(fixture.app.state.board.live_thoughts().len(), 1);
    fixture.input(UiInput::Key(UiKey::Move {
        movement: CursorMovement::DocumentStart,
        extend_selection: false,
    }));
    fixture.input(visual(CursorMovement::VisualUp, false));
    fixture.input(visual(CursorMovement::VisualUp, false));

    let live = fixture.app.state.board.live_thoughts();
    assert_eq!(live.len(), 2);
    assert_eq!(live[0].content, "");
    assert_eq!(live[1].id, source);
}

#[test]
fn edit_top_flushes_source_before_create_and_empty_top_does_not_stack() {
    let mut fixture = Fixture::new();
    durable_thought(&mut fixture, "source");
    fixture.input(UiInput::Key(UiKey::Enter));
    fixture.input(UiInput::Key(UiKey::Character('!')));
    fixture.input(UiInput::Key(UiKey::Move {
        movement: CursorMovement::DocumentStart,
        extend_selection: false,
    }));
    fixture.input(visual(CursorMovement::VisualUp, false));
    let effects = fixture.effects(visual(CursorMovement::VisualUp, false));

    assert_eq!(effects.len(), 1);
    assert_eq!(
        fixture.app.state.board.live_thoughts()[1].content,
        "source!"
    );
    for _ in 0..6 {
        fixture.input(visual(CursorMovement::VisualUp, false));
    }
    assert_eq!(fixture.app.state.board.live_thoughts().len(), 2);
}

#[test]
fn edit_plain_k_remains_text_and_empty_boards_ignore_up() {
    let mut fixture = Fixture::new();
    fixture.input(visual(CursorMovement::VisualUp, false));
    fixture.input(visual(CursorMovement::VisualUp, false));
    assert!(fixture.app.state.board.live_thoughts().is_empty());
    fixture.input(UiInput::Key(UiKey::Escape));
    fixture.input(visual(CursorMovement::VisualUp, false));
    fixture.input(visual(CursorMovement::VisualUp, false));
    assert!(fixture.app.state.board.live_thoughts().is_empty());

    durable_thought(&mut fixture, "text");
    fixture.input(UiInput::Key(UiKey::Enter));
    fixture.input(UiInput::Key(UiKey::Move {
        movement: CursorMovement::DocumentStart,
        extend_selection: false,
    }));
    fixture.input(UiInput::Key(UiKey::Character('k')));
    fixture.input(UiInput::Key(UiKey::Character('k')));
    assert_eq!(fixture.app.state.board.live_thoughts().len(), 1);
    assert_eq!(
        fixture.app.editor_snapshot().expect("editor").content,
        "kktext"
    );
}

#[test]
fn top_creation_preserves_collapse_and_clears_contiguous_selection() {
    let mut fixture = Fixture::new();
    for content in ["first", "second", "third"] {
        durable_thought(&mut fixture, content);
    }
    fixture.input(visual(CursorMovement::VisualUp, true));
    fixture.input(visual(CursorMovement::VisualUp, true));
    fixture.input(UiInput::Key(UiKey::Character('c')));
    let original = fixture
        .app
        .state
        .board
        .live_thoughts()
        .iter()
        .map(|thought| (thought.id, thought.presentation))
        .collect::<Vec<_>>();

    fixture.input(visual(CursorMovement::VisualUp, false));
    fixture.input(UiInput::Key(UiKey::Character('k')));

    let live = fixture.app.state.board.live_thoughts();
    assert_eq!(
        live.iter()
            .skip(1)
            .map(|thought| (thought.id, thought.presentation))
            .collect::<Vec<_>>(),
        original
    );
    assert!(
        live.iter()
            .all(|thought| !fixture.app.thought_selected(thought.id))
    );
}

#[test]
fn failed_persistence_rejects_top_creation_without_partial_state() {
    let mut fixture = Fixture::new();
    let sequence = fixture.paste("first");
    fixture.input(UiInput::Key(UiKey::Escape));
    fixture.app.acknowledge_persistence(sequence, false);

    fixture.input(visual(CursorMovement::VisualUp, false));
    assert!(
        fixture
            .effects(UiInput::Key(UiKey::Character('k')))
            .is_empty()
    );
    let live = fixture.app.state.board.live_thoughts();
    assert_eq!(live.len(), 1);
    assert_eq!(live[0].content, "first");
    assert_eq!(fixture.app.interaction_mode(), InteractionMode::Board);
}
