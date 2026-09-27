use super::*;
use proqi::domain::{BoardMutation, ThoughtId};
use proqi::domain::{Separator, Thought, ThoughtPosition};

fn selected_fixture() -> (Fixture, ThoughtId, ThoughtId) {
    let mut fixture = Fixture::new();
    for content in ["first", "second", "third", "fourth", "fifth"] {
        durable_thought(&mut fixture, content);
    }
    let thoughts = fixture.app.state.board.live_thoughts();
    let third = thoughts[2].id;
    let fifth = thoughts[4].id;
    fixture.input(key_input(UiKey::Character(' ')));
    fixture.input(key_input(UiKey::Character('k')));
    fixture.input(key_input(UiKey::Character('k')));
    fixture.input(key_input(UiKey::Character(' ')));
    (fixture, third, fifth)
}

fn order(fixture: &Fixture) -> Vec<&str> {
    fixture
        .app
        .state
        .board
        .live_thoughts()
        .iter()
        .map(|thought| thought.content.as_str())
        .collect()
}

#[test]
fn disjoint_selected_runs_exchange_each_neighbor_for_arrow_and_configured_k() {
    for key in [
        UiKey::PrimaryShiftMove {
            movement: CursorMovement::VisualUp,
        },
        UiKey::PrimaryShiftCharacter('K'),
        UiKey::PrimaryShiftCharacter('k'),
    ] {
        let (mut fixture, third, fifth) = selected_fixture();
        let focus = fixture.app.state.focused_item;
        let effects = fixture.effects(key_input(key));
        assert!(
            matches!(effects.as_slice(), [Effect::CommitBoardOperation(operation)]
            if matches!(&operation.forward, BoardMutation::Batch { mutations } if mutations.len() == 2))
        );
        assert_eq!(
            order(&fixture),
            ["first", "third", "second", "fifth", "fourth"]
        );
        assert_eq!(fixture.app.state.focused_item, focus);
        assert!(fixture.app.thought_selected(third));
        assert!(fixture.app.thought_selected(fifth));
        assert_eq!(
            fixture
                .app
                .state
                .board
                .live_thoughts()
                .iter()
                .filter(|thought| fixture.app.thought_selected(thought.id))
                .count(),
            2
        );
        fixture.input(key_input(UiKey::Undo));
        assert_eq!(
            order(&fixture),
            ["first", "second", "third", "fourth", "fifth"]
        );
        fixture.input(key_input(UiKey::Redo));
        assert_eq!(
            order(&fixture),
            ["first", "third", "second", "fifth", "fourth"]
        );
    }
}

#[test]
fn mixed_selection_spacing_changes_only_eligible_thoughts_in_one_history_unit() {
    let mut ids = FakeIdGenerator::new(1_725_991_000_000);
    let at = Timestamp::from_millis(10);
    let session = Session::new(
        ids.session_id(),
        std::env::temp_dir().join("proqi-selected-reflow"),
        at,
    )
    .expect("session");
    let first = Thought::new(
        ids.thought_id(),
        session.id,
        "first   line".to_owned(),
        ThoughtPosition::new(0),
        at,
    );
    let separator = Separator::new(ids.separator_id(), session.id, ThoughtPosition::new(1), at);
    let second = Thought::new(
        ids.thought_id(),
        session.id,
        "second   line".to_owned(),
        ThoughtPosition::new(2),
        at,
    );
    let mut state = AppState::new(
        SessionBoard::with_separators(
            session,
            vec![first.clone(), second.clone()],
            vec![separator.clone()],
        )
        .expect("board"),
    );
    state.focused_item = Some(second.id.into());
    let mut fixture = Fixture {
        app: BoardApp::new(state, proqi::adapters::editor::RopeEditorFactory),
        ids,
        clock: FakeClock::new(Timestamp::from_millis(20)),
    };
    fixture.input(key_input(UiKey::Character(' ')));
    fixture.input(key_input(UiKey::Character('k')));
    fixture.input(key_input(UiKey::Character(' ')));
    fixture.input(key_input(UiKey::Character('k')));
    fixture.input(key_input(UiKey::Character(' ')));
    let focus = fixture.app.state.focused_item;
    let effects = fixture.effects(key_input(UiKey::Character('f')));
    assert!(
        matches!(effects.as_slice(), [Effect::CommitBoardOperation(operation)]
        if matches!(&operation.forward, BoardMutation::Batch { mutations } if mutations.len() == 2))
    );
    assert_eq!(order(&fixture), ["first line", "second line"]);
    assert_eq!(fixture.app.state.focused_item, focus);
    assert!(fixture.app.item_selected(separator.id.into()));
    fixture.input(key_input(UiKey::Undo));
    assert_eq!(order(&fixture), ["first   line", "second   line"]);
    fixture.input(key_input(UiKey::Redo));
    assert_eq!(order(&fixture), ["first line", "second line"]);
}

#[test]
fn selected_reorder_keeps_focus_and_selection_visible_in_a_shallow_pane() {
    let (mut fixture, third, fifth) = selected_fixture();
    fixture.input(key_input(UiKey::PrimaryShiftMove {
        movement: CursorMovement::VisualUp,
    }));
    assert!(fixture.app.thought_selected(third));
    assert!(fixture.app.thought_selected(fifth));
    insta::assert_snapshot!(super::snapshot_support::snapshot_buffer(
        draw(&mut fixture, 36, 8).backend().buffer()
    ));
}

#[test]
fn clean_selected_thoughts_produces_no_operation_when_spacing_is_already_clean() {
    let (mut fixture, third, fifth) = selected_fixture();
    let focus = fixture.app.state.focused_item;
    let effects = fixture.effects(key_input(UiKey::Character('f')));
    assert!(effects.is_empty());
    assert_eq!(fixture.app.state.focused_item, focus);
    assert!(fixture.app.thought_selected(third));
    assert!(fixture.app.thought_selected(fifth));
    assert_eq!(
        order(&fixture),
        ["first", "second", "third", "fourth", "fifth"]
    );
}
