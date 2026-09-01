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
    }
    fixture.input(UiInput::Key(UiKey::Character(' ')));
    fixture.input(UiInput::Key(UiKey::Character('k')));
    fixture.input(UiInput::Key(UiKey::Character(' ')));
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
            .effects(UiInput::Key(UiKey::PrimaryCharacter('J')))
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

fn selected_contents(fixture: &Fixture) -> Vec<&str> {
    fixture
        .app
        .state
        .board
        .live_thoughts()
        .into_iter()
        .filter(|thought| fixture.app.thought_selected(thought.id))
        .map(|thought| thought.content.as_str())
        .collect()
}

fn range_move(fixture: &mut Fixture, movement: CursorMovement) {
    fixture.input(UiInput::Key(UiKey::Move {
        movement,
        extend_selection: true,
    }));
}

#[test]
fn shifted_arrows_shrink_and_reverse_around_a_stable_anchor() {
    let mut fixture = Fixture::new();
    for content in ["first", "second", "third", "fourth", "fifth"] {
        fixture.paste(content);
        fixture.input(UiInput::Key(UiKey::Escape));
    }
    fixture.input(UiInput::Key(UiKey::Character('k')));
    fixture.input(UiInput::Key(UiKey::Character('k')));

    range_move(&mut fixture, CursorMovement::VisualUp);
    assert_eq!(selected_contents(&fixture), ["second", "third"]);
    range_move(&mut fixture, CursorMovement::VisualDown);
    assert_eq!(selected_contents(&fixture), ["third"]);
    range_move(&mut fixture, CursorMovement::VisualDown);
    assert_eq!(selected_contents(&fixture), ["third", "fourth"]);
    range_move(&mut fixture, CursorMovement::VisualDown);
    assert_eq!(selected_contents(&fixture), ["third", "fourth", "fifth"]);
    range_move(&mut fixture, CursorMovement::VisualUp);
    assert_eq!(selected_contents(&fixture), ["third", "fourth"]);

    let focused = fixture.app.state.focused_thought.expect("range endpoint");
    assert_eq!(
        fixture
            .app
            .state
            .board
            .thought(focused)
            .expect("thought")
            .content,
        "fourth"
    );
}

#[test]
fn starting_a_range_replaces_arbitrary_selection_and_space_returns_to_toggle_selection() {
    let mut fixture = Fixture::new();
    for content in ["first", "second", "third", "fourth"] {
        fixture.paste(content);
        fixture.input(UiInput::Key(UiKey::Escape));
    }
    fixture.input(UiInput::Key(UiKey::Character(' ')));
    fixture.input(UiInput::Key(UiKey::Character('k')));
    fixture.input(UiInput::Key(UiKey::Character('k')));
    fixture.input(UiInput::Key(UiKey::Character(' ')));
    assert_eq!(selected_contents(&fixture), ["second", "fourth"]);

    range_move(&mut fixture, CursorMovement::VisualDown);
    assert_eq!(selected_contents(&fixture), ["second", "third"]);

    fixture.input(UiInput::Key(UiKey::Character(' ')));
    assert_eq!(selected_contents(&fixture), ["second"]);
    fixture.input(UiInput::Key(UiKey::Character('j')));
    assert_eq!(selected_contents(&fixture), ["second"]);
}

#[test]
fn range_latch_extends_with_repeated_arrows_and_jk_without_wrapping_or_insertion() {
    let mut fixture = Fixture::new();
    for content in ["first", "second", "third"] {
        fixture.paste(content);
        fixture.input(UiInput::Key(UiKey::Escape));
    }
    fixture.input(UiInput::Key(UiKey::Character('k')));
    fixture.input(UiInput::Key(UiKey::Character('v')));

    fixture.input(UiInput::Key(UiKey::Character('k')));
    fixture.input(UiInput::Key(UiKey::Character('k')));
    assert_eq!(selected_contents(&fixture), ["first", "second"]);
    assert!(!fixture.app.insertion_focused());

    fixture.input(UiInput::Key(UiKey::Character('j')));
    fixture.input(UiInput::Key(UiKey::Move {
        movement: CursorMovement::VisualDown,
        extend_selection: false,
    }));
    fixture.input(UiInput::Key(UiKey::Move {
        movement: CursorMovement::VisualDown,
        extend_selection: false,
    }));
    assert_eq!(selected_contents(&fixture), ["second", "third"]);
    assert!(!fixture.app.insertion_focused());
}

#[test]
fn escape_and_edit_entry_clear_range_and_latch_consistently() {
    let mut fixture = Fixture::new();
    for content in ["first", "second", "third"] {
        fixture.paste(content);
        fixture.input(UiInput::Key(UiKey::Escape));
    }
    fixture.input(UiInput::Key(UiKey::Character('v')));
    fixture.input(UiInput::Key(UiKey::Character('k')));
    fixture.input(UiInput::Key(UiKey::Escape));
    assert!(selected_contents(&fixture).is_empty());

    fixture.input(UiInput::Key(UiKey::Character('j')));
    assert!(selected_contents(&fixture).is_empty());
    fixture.input(UiInput::Key(UiKey::Character('v')));
    fixture.input(UiInput::Key(UiKey::Character('k')));
    fixture.input(UiInput::Key(UiKey::Enter));
    assert!(selected_contents(&fixture).is_empty());
    assert!(matches!(
        fixture.app.interaction_mode(),
        proqi::application::InteractionMode::Edit { .. }
    ));
}

#[test]
fn range_survives_reflow_and_shift_click_uses_current_hit_geometry_with_unicode() {
    let mut fixture = Fixture::new();
    for content in ["alpha", "Grüße 👩‍💻", "第二行", "omega"] {
        fixture.paste(content);
        fixture.input(UiInput::Key(UiKey::Escape));
    }
    fixture.input(UiInput::Key(UiKey::Character('k')));
    fixture.input(UiInput::Key(UiKey::Character('k')));
    let _wide = fixture.app.prepare_frame(Rect::new(0, 0, 60, 14));
    let narrow = fixture.app.prepare_frame(Rect::new(0, 0, 24, 14));
    let target = narrow.thoughts[3].text_area;
    fixture.input(UiInput::Pointer(PointerInput {
        column: target.x,
        row: target.y,
        kind: PointerKind::Down(PointerButton::Left),
        extend_selection: true,
    }));

    assert_eq!(selected_contents(&fixture), ["Grüße 👩‍💻", "第二行", "omega"]);
    assert!(matches!(
        fixture.app.interaction_mode(),
        proqi::application::InteractionMode::Board
    ));
    let copy = fixture.effects(UiInput::Key(UiKey::Copy));
    assert!(matches!(
        copy.as_slice(),
        [Effect::WriteClipboard { content, .. }]
            if content == "Grüße 👩‍💻\n\n第二行\n\nomega"
    ));
}

#[test]
fn latch_click_is_a_modifier_free_mouse_fallback_and_modal_entry_releases_the_latch() {
    let mut fixture = Fixture::new();
    for content in ["first", "second", "third"] {
        fixture.paste(content);
        fixture.input(UiInput::Key(UiKey::Escape));
    }
    fixture.input(UiInput::Key(UiKey::Character('v')));
    let layout = fixture.app.prepare_frame(Rect::new(0, 0, 50, 12));
    let target = layout.thoughts[0].text_area;
    fixture.pointer(target.x, target.y, PointerKind::Down(PointerButton::Left));
    assert_eq!(selected_contents(&fixture), ["first", "second", "third"]);

    fixture.input(UiInput::Key(UiKey::Character('?')));
    fixture.input(UiInput::Key(UiKey::Escape));
    fixture.input(UiInput::Key(UiKey::Character('j')));
    assert!(selected_contents(&fixture).is_empty());
}

#[test]
fn search_focus_transition_clears_an_anchored_range() {
    let mut fixture = Fixture::new();
    for content in ["needle first", "second", "third"] {
        fixture.paste(content);
        fixture.input(UiInput::Key(UiKey::Escape));
    }
    fixture.input(UiInput::Key(UiKey::Character('v')));
    fixture.input(UiInput::Key(UiKey::Character('k')));
    assert_eq!(selected_contents(&fixture), ["second", "third"]);

    fixture.input(UiInput::Key(UiKey::Character('/')));
    for character in "needle".chars() {
        fixture.input(UiInput::Key(UiKey::Character(character)));
    }
    fixture.input(UiInput::Key(UiKey::Enter));

    assert!(selected_contents(&fixture).is_empty());
    let focused = fixture.app.state.focused_thought.expect("search focus");
    assert_eq!(
        fixture
            .app
            .state
            .board
            .thought(focused)
            .expect("thought")
            .content,
        "needle first"
    );
}

#[test]
fn range_latch_uses_the_remappable_board_binding() {
    let mut settings = UiSettings::default();
    settings.keybindings.range_select = 'b';
    let mut fixture = Fixture::with_settings(settings);
    for content in ["first", "second"] {
        fixture.paste(content);
        fixture.input(UiInput::Key(UiKey::Escape));
    }

    fixture.input(UiInput::Key(UiKey::Character('v')));
    fixture.input(UiInput::Key(UiKey::Character('k')));
    assert!(selected_contents(&fixture).is_empty());
    fixture.input(UiInput::Key(UiKey::Character('j')));
    fixture.input(UiInput::Key(UiKey::Character('b')));
    fixture.input(UiInput::Key(UiKey::Character('k')));
    assert_eq!(selected_contents(&fixture), ["first", "second"]);
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

#[test]
fn duplicate_preserves_existing_shortcut_metadata_without_reauthoring_it() {
    let annotation: ContentAnnotation = serde_json::from_value(serde_json::json!({
        "start": 6,
        "end": 11,
        "kind": { "kind": "shortcut_emphasis" }
    }))
    .expect("structurally valid durable fixture");
    let mut fixture = Fixture::with_annotated_thought("Press Enter", vec![annotation.clone()]);

    let effects = fixture.effects(UiInput::Key(UiKey::Duplicate));

    assert!(matches!(
        effects.as_slice(),
        [Effect::CommitBoardOperation(operation)]
            if operation.kind == proqi::domain::BoardOperationKind::Duplicate
    ));
    let thoughts = fixture.app.state.board.live_thoughts();
    assert_eq!(thoughts.len(), 2);
    assert_eq!(thoughts[0].content, thoughts[1].content);
    assert_eq!(
        thoughts[0].annotations.as_slice(),
        std::slice::from_ref(&annotation)
    );
    assert_eq!(thoughts[1].annotations, [annotation]);
}
