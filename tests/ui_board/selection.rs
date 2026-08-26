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
fn selected_fully_visible_thoughts_collapse_as_one_undo_step_but_cannot_reorder() {
    let mut fixture = Fixture::new();
    for content in ["first", "second"] {
        fixture.paste(content);
        fixture.input(UiInput::Key(UiKey::Escape));
        fixture.input(UiInput::Key(UiKey::Character(' ')));
    }
    let _layout = fixture.app.prepare_frame(Rect::new(0, 0, 50, 12));

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
            .all(|thought| thought.presentation == proqi::domain::ThoughtPresentation::Collapsed)
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
            .all(|thought| thought.presentation == proqi::domain::ThoughtPresentation::Automatic)
    );
}

#[test]
fn escape_clears_the_complete_board_selection() {
    let mut fixture = Fixture::new();
    for content in ["first", "second"] {
        fixture.paste(content);
        fixture.input(UiInput::Key(UiKey::Escape));
        fixture.input(UiInput::Key(UiKey::Character(' ')));
    }

    fixture.input(UiInput::Key(UiKey::Escape));

    for thought in fixture.app.state.board.live_thoughts() {
        assert!(!fixture.app.thought_selected(thought.id));
    }
}

#[test]
fn entering_edit_mode_clears_selection_and_hover_cannot_replace_it() {
    let mut fixture = Fixture::new();
    fixture.paste("selected");
    fixture.input(UiInput::Key(UiKey::Escape));
    fixture.input(UiInput::Key(UiKey::Character(' ')));
    let layout = fixture.app.prepare_frame(Rect::new(0, 0, 50, 12));
    let area = layout.thoughts[0].text_area;
    fixture.pointer(area.x, area.y, PointerKind::Move);
    assert!(fixture.app.hovered().is_none());

    fixture.input(UiInput::Key(UiKey::Enter));

    assert!(matches!(
        fixture.app.interaction_mode(),
        proqi::application::InteractionMode::Edit { .. }
    ));
    assert!(!fixture.app.thought_selected(layout.thoughts[0].thought_id));
}

#[test]
fn duplicate_copies_selection_below_its_range_as_one_undoable_operation() {
    let mut fixture = Fixture::new();
    for content in ["first", "second", "third"] {
        fixture.paste(content);
        fixture.input(UiInput::Key(UiKey::Escape));
    }
    fixture.input(UiInput::Key(UiKey::Character(' ')));
    fixture.input(UiInput::Key(UiKey::Character('k')));
    fixture.input(UiInput::Key(UiKey::Character(' ')));

    let effects = fixture.effects(UiInput::Key(UiKey::Duplicate));

    assert!(matches!(
        effects.as_slice(),
        [Effect::CommitBoardOperation(operation)]
            if operation.kind == proqi::domain::BoardOperationKind::Duplicate
    ));
    let thoughts = fixture.app.state.board.live_thoughts();
    assert_eq!(
        thoughts
            .iter()
            .map(|thought| thought.content.as_str())
            .collect::<Vec<_>>(),
        ["first", "second", "third", "second", "third"]
    );
    assert!(fixture.app.thought_selected(thoughts[3].id));
    assert!(fixture.app.thought_selected(thoughts[4].id));

    fixture.input(UiInput::Key(UiKey::Undo));
    assert_eq!(fixture.app.state.board.live_thoughts().len(), 3);
}
