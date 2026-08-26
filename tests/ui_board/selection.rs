use super::*;

#[test]
fn selected_thoughts_copy_and_delete_in_board_order_as_one_undo_step() {
    let mut fixture = Fixture::new();
    for content in ["first", "second", "third"] {
        fixture.paste(content);
        fixture.input(UiInput::Key(UiKey::Escape));
    }
    fixture.input(UiInput::Key(UiKey::Character(' ')));
    fixture.input(UiInput::Key(UiKey::Character('k')));
    fixture.input(UiInput::Key(UiKey::Character(' ')));

    let copy = fixture.effects(UiInput::Key(UiKey::Copy));
    assert!(matches!(
        copy.as_slice(),
        [Effect::WriteClipboard { content, .. }] if content == "second\n\nthird"
    ));
    let delete = fixture.effects(UiInput::Key(UiKey::Character('d')));
    assert!(matches!(
        delete.as_slice(),
        [Effect::CommitBoardOperation(operation)]
            if matches!(&operation.forward, proqi::domain::BoardMutation::Batch { mutations }
                if mutations.len() == 2)
    ));
    assert_eq!(fixture.app.state.board.live_thoughts()[0].content, "first");

    fixture.input(UiInput::Key(UiKey::Undo));
    let restored = fixture.app.state.board.live_thoughts();
    assert_eq!(restored.len(), 3);
    assert_eq!(restored[1].content, "second");
    assert_eq!(restored[2].content, "third");
}

#[test]
fn selected_thoughts_collapse_as_one_undo_step_but_cannot_reorder() {
    let mut fixture = Fixture::new();
    for content in ["first", "second"] {
        fixture.paste(content);
        fixture.input(UiInput::Key(UiKey::Escape));
        fixture.input(UiInput::Key(UiKey::Character(' ')));
    }

    let collapse = fixture.effects(UiInput::Key(UiKey::Character('c')));
    assert!(matches!(
        collapse.as_slice(),
        [Effect::CommitBoardOperation(_)]
    ));
    assert!(
        fixture
            .app
            .state
            .board
            .live_thoughts()
            .iter()
            .all(|thought| thought.collapsed)
    );
    assert!(
        fixture
            .effects(UiInput::Key(UiKey::Character('J')))
            .is_empty()
    );

    fixture.input(UiInput::Key(UiKey::Undo));
    assert!(
        fixture
            .app
            .state
            .board
            .live_thoughts()
            .iter()
            .all(|thought| !thought.collapsed)
    );
}
