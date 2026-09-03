use super::*;

use proqi::application::InteractionMode;

#[test]
fn fresh_empty_board_starts_in_compose_without_durable_state() {
    let mut fixture = Fixture::new();
    let sequence = fixture.app.state.board.session.last_durable_sequence;

    assert_eq!(fixture.app.interaction_mode(), InteractionMode::Compose);
    assert!(fixture.app.compose_prompt_visible());
    assert!(fixture.app.state.board.live_thoughts().is_empty());
    assert!(fixture.app.state.board_history().is_empty());
    assert_eq!(
        fixture
            .app
            .editor_snapshot()
            .expect("compose editor")
            .content,
        ""
    );

    for input in [
        UiInput::Resize {
            width: 12,
            height: 4,
        },
        UiInput::HostFocusGained,
        UiInput::Resize {
            width: 100,
            height: 30,
        },
    ] {
        let _effects = fixture.effects(input);
        let terminal = draw(&mut fixture, 40, 8);
        let rendered = text(terminal.backend().buffer());
        assert!(rendered.contains("+ Start typing"));
        assert!(!rendered.contains("compose"));
    }

    assert_eq!(fixture.app.interaction_mode(), InteractionMode::Compose);
    assert!(fixture.app.state.board.live_thoughts().is_empty());
    assert!(fixture.app.state.board_history().is_empty());
    assert_eq!(
        fixture.app.state.board.session.last_durable_sequence,
        sequence
    );
}

#[test]
fn passive_prompt_click_engages_compose_and_focus_loss_collapses_it() {
    let mut fixture = Fixture::new();
    let sequence = fixture.app.state.board.session.last_durable_sequence;
    let layout = fixture.app.prepare_frame(Rect::new(0, 0, 40, 8));
    assert!(layout.compose.is_none());
    let prompt = layout.insert.expect("passive prompt row");

    fixture.pointer(
        prompt.x.saturating_add(prompt.width / 2),
        prompt.y,
        PointerKind::Down(PointerButton::Left),
    );

    assert!(fixture.app.compose_editor_visible());
    let engaged = fixture.app.prepare_frame(Rect::new(0, 0, 40, 8));
    assert!(engaged.compose.is_some());
    assert!(engaged.insert.is_none());
    assert!(fixture.effects(UiInput::HostFocusLost).is_empty());
    assert!(fixture.app.compose_prompt_visible());
    assert!(fixture.app.state.board.live_thoughts().is_empty());
    assert!(fixture.app.state.board_history().is_empty());
    assert_eq!(
        fixture.app.state.board.session.last_durable_sequence,
        sequence
    );
}

#[test]
fn first_conflicting_character_is_one_exact_populated_create() {
    let mut fixture = Fixture::new();
    let effects = fixture.effects(UiInput::Key(UiKey::Character('n')));
    let [Effect::CommitBoardOperation(operation)] = effects.as_slice() else {
        panic!("first semantic input must be one create operation");
    };
    assert_eq!(operation.kind, proqi::domain::BoardOperationKind::Create);
    assert_eq!(fixture.app.state.board.live_thoughts()[0].content, "n");
    assert_eq!(
        fixture.app.interaction_mode(),
        InteractionMode::Edit {
            thought_id: fixture.app.state.board.live_thoughts()[0].id,
        }
    );

    for character in "qs:?jk界e\u{301}👩‍💻".chars() {
        assert!(
            fixture
                .effects(UiInput::Key(UiKey::Character(character)))
                .is_empty()
        );
    }
    assert_eq!(
        fixture
            .app
            .editor_snapshot()
            .expect("promoted editor")
            .content,
        "nqs:?jk界e\u{301}👩‍💻"
    );
    assert_eq!(fixture.app.state.board_history().len(), 1);
}

#[test]
fn exact_and_annotated_paste_materialize_through_the_canonical_create() {
    let content = "nqs:?jk\tGrüße\r\n界\nעברית\0\u{1f469}\u{200d}\u{1f4bb}";
    let mut fixture = Fixture::new();
    let effects = fixture.effects(UiInput::Paste(content.to_owned()));
    let [Effect::CommitBoardOperation(operation)] = effects.as_slice() else {
        panic!("exact paste must be one create operation");
    };
    let proqi::domain::BoardMutation::AddThought { thought } = &operation.forward else {
        panic!("expected populated create payload");
    };
    assert_eq!(thought.content, content);
    assert_eq!(
        fixture.app.editor_snapshot().expect("editor").content,
        content
    );

    let mut annotated = Fixture::new();
    let path = "/tmp/context file.txt";
    let annotation = ContentAnnotation {
        start: 0,
        end: path.len(),
        kind: ContentAnnotationKind::Attachment {
            image: false,
            display_name: "context file.txt".to_owned(),
        },
    };
    let effects = annotated.effects(UiInput::PasteAnnotated(
        PastePayload::annotated(path.to_owned(), vec![annotation.clone()])
            .expect("valid attachment payload"),
    ));
    assert_eq!(
        effects
            .iter()
            .filter(|effect| matches!(effect, Effect::CommitBoardOperation(_)))
            .count(),
        1
    );
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, Effect::CheckAttachments(_)))
    );
    let thought = &annotated.app.state.board.live_thoughts()[0];
    assert_eq!(thought.content, path);
    assert_eq!(thought.annotations, vec![annotation]);
}

#[test]
fn editor_only_intentions_do_not_materialize_until_they_change_content() {
    let mut fixture = Fixture::new();
    for key in [
        UiKey::Backspace,
        UiKey::Delete,
        UiKey::SelectAll,
        UiKey::Move {
            movement: CursorMovement::DocumentEnd,
            extend_selection: true,
        },
    ] {
        assert!(fixture.effects(UiInput::Key(key)).is_empty());
    }
    assert!(fixture.app.state.board.live_thoughts().is_empty());
    assert!(fixture.app.state.board_history().is_empty());

    let effects = fixture.effects(UiInput::Key(UiKey::Enter));
    assert!(matches!(
        effects.as_slice(),
        [Effect::CommitBoardOperation(_)]
    ));
    assert_eq!(fixture.app.state.board.live_thoughts()[0].content, "\n");
}

#[test]
fn durable_thought_edited_back_to_empty_remains_an_editable_blank() {
    let mut fixture = Fixture::new();
    fixture.paste("x");
    assert!(fixture.effects(UiInput::Key(UiKey::Backspace)).is_empty());

    assert_eq!(fixture.app.state.board.live_thoughts().len(), 1);
    assert_eq!(
        fixture
            .app
            .editor_snapshot()
            .expect("durable editor")
            .content,
        ""
    );
    assert!(matches!(
        fixture.app.interaction_mode(),
        InteractionMode::Edit { .. }
    ));
    assert!(!fixture.app.compose_prompt_visible());
}

#[test]
fn edit_footer_uses_only_truthful_native_chords() {
    let mut fixture = Fixture::new();
    fixture.paste("footer contract");

    let terminal = draw(&mut fixture, 100, 8);
    let rendered = text(terminal.backend().buffer());
    let primary = if cfg!(target_os = "macos") {
        "Cmd+"
    } else {
        "Ctrl+"
    };
    for label in [
        format!("{primary}C Copy"),
        format!("{primary}X Cut"),
        format!("{primary}Z Undo"),
    ] {
        assert!(rendered.contains(&label), "missing {label:?}: {rendered:?}");
    }
    for misleading in ["y Copy", "x Cut", "u Undo", ": Commands", "? Shortcuts"] {
        assert!(
            !rendered.contains(misleading),
            "misleading {misleading:?}: {rendered:?}"
        );
    }
    assert!(rendered.contains("Esc Board"));
}

#[test]
fn escape_is_a_sticky_board_choice_and_explicit_insertion_returns_to_compose() {
    let mut fixture = Fixture::new();
    assert!(fixture.effects(UiInput::Key(UiKey::Escape)).is_empty());
    assert_eq!(fixture.app.interaction_mode(), InteractionMode::Board);

    for input in [
        UiInput::Resize {
            width: 16,
            height: 4,
        },
        UiInput::HostFocusGained,
    ] {
        let _effects = fixture.effects(input);
        assert_eq!(fixture.app.interaction_mode(), InteractionMode::Board);
    }
    fixture.input(UiInput::Key(UiKey::Character('?')));
    assert!(fixture.app.help);
    fixture.input(UiInput::Key(UiKey::Escape));
    fixture.input(UiInput::Key(UiKey::Enter));

    assert_eq!(fixture.app.interaction_mode(), InteractionMode::Compose);
    assert!(fixture.app.compose_editor_visible());
    assert!(fixture.app.state.board.live_thoughts().is_empty());
    assert!(fixture.app.state.board_history().is_empty());
}

#[test]
fn failed_empty_history_moves_do_not_override_the_sticky_board_choice() {
    let mut fixture = Fixture::new();
    fixture.input(UiInput::Key(UiKey::Escape));

    for key in [UiKey::Undo, UiKey::Redo] {
        assert!(fixture.effects(UiInput::Key(key)).is_empty());
        assert_eq!(fixture.app.interaction_mode(), InteractionMode::Board);
        assert!(fixture.app.state.board.live_thoughts().is_empty());
        assert!(fixture.app.state.board_history().is_empty());
    }
}

#[test]
fn escape_exposes_screenshot_listening_before_any_prompt_exists() {
    let mut fixture = Fixture::new();
    fixture.input(UiInput::Key(UiKey::Escape));

    let effects = fixture.effects(UiInput::Key(UiKey::Character('i')));

    assert!(matches!(
        effects.as_slice(),
        [Effect::Screenshot(
            proqi::application::ScreenshotIntent::Enable
        )]
    ));
    assert_eq!(fixture.app.interaction_mode(), InteractionMode::Board);
    assert!(fixture.app.state.board.live_thoughts().is_empty());
    assert!(fixture.app.state.board_history().is_empty());
}

#[test]
fn empty_compose_submit_chords_are_no_ops() {
    let mut fixture = Fixture::new();
    for key in [UiKey::Submit, UiKey::SubmitKeep] {
        assert!(fixture.effects(UiInput::Key(key)).is_empty());
    }
    assert_eq!(fixture.app.interaction_mode(), InteractionMode::Compose);
    assert!(fixture.app.compose_prompt_visible());
    assert!(fixture.app.state.board.live_thoughts().is_empty());
    assert!(fixture.app.state.board_history().is_empty());
}

#[test]
fn deliberate_final_deletion_enters_compose_and_undo_is_available_via_board() {
    let mut fixture = Fixture::new();
    fixture.paste("remove me");
    fixture.input(UiInput::Key(UiKey::Escape));
    fixture.input(UiInput::Key(UiKey::Character('d')));
    assert_eq!(fixture.app.interaction_mode(), InteractionMode::Compose);
    assert!(fixture.app.compose_prompt_visible());

    fixture.input(UiInput::Key(UiKey::Escape));
    fixture.input(UiInput::Key(UiKey::Undo));
    assert_eq!(
        fixture.app.state.board.live_thoughts()[0].content,
        "remove me"
    );
    assert_eq!(fixture.app.interaction_mode(), InteractionMode::Board);

    fixture.input(UiInput::Key(UiKey::Redo));
    assert!(fixture.app.state.board.live_thoughts().is_empty());
    assert_eq!(fixture.app.interaction_mode(), InteractionMode::Compose);
    assert!(fixture.app.compose_prompt_visible());
}
