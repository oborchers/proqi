use super::{Fixture, draw};

use proqi::{
    application::{ClipboardIntent, Effect, FailureCode, InteractionMode},
    ports::editor::CursorMovement,
    ui::{UiInput, UiKey},
};

#[test]
fn board_cut_waits_for_clipboard_success_and_copy_preserves_exact_content() {
    let mut fixture = Fixture::new();
    let sequence = fixture.paste(" exact\r\n界 ");
    fixture.app.acknowledge_persistence(sequence, true);
    fixture.input(UiInput::Key(UiKey::Escape));

    let copy = fixture.effects(UiInput::Key(UiKey::Copy));
    let [
        Effect::WriteClipboard {
            request_id,
            intent: ClipboardIntent::Copy,
            content,
            ..
        },
    ] = copy.as_slice()
    else {
        panic!("expected copy effect");
    };
    assert_eq!(content, " exact\r\n界 ");
    assert!(
        fixture
            .app
            .complete_clipboard_write(*request_id, Ok(()), &mut fixture.ids, &fixture.clock)
            .is_empty()
    );
    assert_eq!(fixture.app.state.board.live_thoughts().len(), 1);

    let failed_cut = fixture.effects(UiInput::Key(UiKey::Cut));
    let [Effect::WriteClipboard { request_id, .. }] = failed_cut.as_slice() else {
        panic!("expected cut effect");
    };
    let failure = fixture.app.complete_clipboard_write(
        *request_id,
        Err(FailureCode::ClipboardFailed),
        &mut fixture.ids,
        &fixture.clock,
    );
    assert!(matches!(failure.as_slice(), [Effect::Notify { .. }]));
    assert_eq!(fixture.app.state.board.live_thoughts().len(), 1);

    let cut = fixture.effects(UiInput::Key(UiKey::Cut));
    let [
        Effect::WriteClipboard {
            request_id,
            intent: ClipboardIntent::Cut,
            ..
        },
    ] = cut.as_slice()
    else {
        panic!("expected cut effect");
    };
    let deletion =
        fixture
            .app
            .complete_clipboard_write(*request_id, Ok(()), &mut fixture.ids, &fixture.clock);
    assert!(matches!(
        deletion.as_slice(),
        [Effect::CommitBoardOperation(_)]
    ));
    assert!(fixture.app.state.board.live_thoughts().is_empty());
    assert_eq!(
        fixture.app.state.mode,
        proqi::application::InteractionMode::Compose
    );
    assert!(fixture.app.editor_snapshot().is_some());

    fixture.input(UiInput::Key(UiKey::Character('n')));
    fixture.input(UiInput::Key(UiKey::Escape));
    assert_eq!(fixture.app.state.board.live_thoughts()[0].content, "n");
}

#[test]
fn editor_cut_is_non_destructive_on_failure_or_changed_selection() {
    let mut fixture = Fixture::new();
    fixture.paste("A界B");
    fixture.input(UiInput::Key(UiKey::Move {
        movement: CursorMovement::GraphemeBack,
        extend_selection: true,
    }));
    let cut = fixture.effects(UiInput::Key(UiKey::Cut));
    let [
        Effect::WriteClipboard {
            request_id,
            content,
            ..
        },
    ] = cut.as_slice()
    else {
        panic!("expected selection write");
    };
    assert_eq!(content, "B");
    fixture.app.complete_clipboard_write(
        *request_id,
        Err(FailureCode::ClipboardFailed),
        &mut fixture.ids,
        &fixture.clock,
    );
    assert_eq!(
        fixture.app.editor_snapshot().expect("editor").content,
        "A界B"
    );

    let cut = fixture.effects(UiInput::Key(UiKey::Cut));
    let [Effect::WriteClipboard { request_id, .. }] = cut.as_slice() else {
        panic!("expected selection write");
    };
    fixture.input(UiInput::Key(UiKey::Move {
        movement: CursorMovement::GraphemeBack,
        extend_selection: false,
    }));
    assert!(
        fixture
            .app
            .complete_clipboard_write(*request_id, Ok(()), &mut fixture.ids, &fixture.clock)
            .is_empty()
    );
    assert_eq!(
        fixture.app.editor_snapshot().expect("editor").content,
        "A界B"
    );
}

#[test]
fn editor_cut_survives_viewport_reflow_when_selection_is_unchanged() {
    let mut fixture = Fixture::new();
    fixture.paste("A long wrapped selection 界B");
    fixture.input(UiInput::Key(UiKey::Move {
        movement: CursorMovement::GraphemeBack,
        extend_selection: true,
    }));
    let cut = fixture.effects(UiInput::Key(UiKey::Cut));
    let [Effect::WriteClipboard { request_id, .. }] = cut.as_slice() else {
        panic!("expected selection write");
    };
    let _terminal = draw(&mut fixture, 10, 5);
    let effects =
        fixture
            .app
            .complete_clipboard_write(*request_id, Ok(()), &mut fixture.ids, &fixture.clock);
    assert!(matches!(effects.as_slice(), [Effect::CommitRevision(_)]));
    assert_eq!(
        fixture.app.editor_snapshot().expect("editor").content,
        "A long wrapped selection 界"
    );
}

#[test]
fn empty_or_failed_clipboard_read_never_creates_a_thought() {
    let mut fixture = Fixture::new();
    for result in [Ok(String::new()), Err(FailureCode::ClipboardFailed)] {
        let read = fixture.effects(UiInput::Key(UiKey::PasteClipboard));
        let [Effect::ReadClipboard { request_id }] = read.as_slice() else {
            panic!("expected clipboard read");
        };
        assert!(
            fixture
                .app
                .complete_clipboard_read(*request_id, result, &mut fixture.ids, &fixture.clock)
                .is_empty()
        );
        assert!(fixture.app.state.board.live_thoughts().is_empty());
    }
}

#[test]
fn compose_clipboard_read_materializes_only_for_its_current_owner() {
    let mut fixture = Fixture::new();
    let read = fixture.effects(UiInput::Key(UiKey::PasteClipboard));
    let [Effect::ReadClipboard { request_id }] = read.as_slice() else {
        panic!("expected clipboard read");
    };

    let effects = fixture.app.complete_clipboard_read(
        *request_id,
        Ok("clipboard prompt".to_owned()),
        &mut fixture.ids,
        &fixture.clock,
    );

    assert!(matches!(
        effects.as_slice(),
        [Effect::CommitBoardOperation(_)]
    ));
    assert_eq!(
        fixture.app.state.board.live_thoughts()[0].content,
        "clipboard prompt"
    );
    assert!(matches!(
        fixture.app.interaction_mode(),
        InteractionMode::Edit { .. }
    ));
}

#[test]
fn delayed_compose_clipboard_success_or_failure_after_escape_is_discarded() {
    for result in [
        Ok("must remain discarded".to_owned()),
        Err(FailureCode::ClipboardFailed),
    ] {
        let mut fixture = Fixture::new();
        let read = fixture.effects(UiInput::Key(UiKey::PasteClipboard));
        let [Effect::ReadClipboard { request_id }] = read.as_slice() else {
            panic!("expected clipboard read");
        };
        fixture.input(UiInput::Key(UiKey::Escape));

        let effects = fixture.app.complete_clipboard_read(
            *request_id,
            result,
            &mut fixture.ids,
            &fixture.clock,
        );

        assert!(effects.is_empty());
        assert_eq!(fixture.app.interaction_mode(), InteractionMode::Board);
        assert!(fixture.app.state.board.live_thoughts().is_empty());
        assert!(fixture.app.state.board_history().is_empty());
        assert!(fixture.app.status_text().is_none());
    }
}

#[test]
fn delayed_clipboard_result_cannot_follow_a_new_compose_generation() {
    let mut fixture = Fixture::new();
    let read = fixture.effects(UiInput::Key(UiKey::PasteClipboard));
    let [Effect::ReadClipboard { request_id }] = read.as_slice() else {
        panic!("expected clipboard read");
    };
    fixture.input(UiInput::Key(UiKey::Escape));
    fixture.input(UiInput::Key(UiKey::Enter));
    assert_eq!(fixture.app.interaction_mode(), InteractionMode::Compose);

    let effects = fixture.app.complete_clipboard_read(
        *request_id,
        Ok("belongs to the previous Compose owner".to_owned()),
        &mut fixture.ids,
        &fixture.clock,
    );

    assert!(effects.is_empty());
    assert!(fixture.app.state.board.live_thoughts().is_empty());
    assert!(fixture.app.state.board_history().is_empty());
    assert!(
        fixture
            .app
            .editor_snapshot()
            .expect("new Compose editor")
            .content
            .is_empty()
    );
}

#[test]
fn compose_clipboard_read_follows_the_same_editor_through_materialization() {
    let mut fixture = Fixture::new();
    let read = fixture.effects(UiInput::Key(UiKey::PasteClipboard));
    let [Effect::ReadClipboard { request_id }] = read.as_slice() else {
        panic!("expected clipboard read");
    };
    assert!(matches!(
        fixture
            .effects(UiInput::Key(UiKey::Character('n')))
            .as_slice(),
        [Effect::CommitBoardOperation(_)]
    ));

    let effects = fixture.app.complete_clipboard_read(
        *request_id,
        Ok("qs:?jk 第二".to_owned()),
        &mut fixture.ids,
        &fixture.clock,
    );

    assert!(matches!(effects.as_slice(), [Effect::CommitRevision(_)]));
    assert_eq!(
        fixture
            .app
            .editor_snapshot()
            .expect("materialized editor")
            .content,
        "nqs:?jk 第二"
    );
    assert!(matches!(
        fixture.app.interaction_mode(),
        InteractionMode::Edit { .. }
    ));
}

#[test]
fn compose_clipboard_failure_remains_visible_after_materialization() {
    let mut fixture = Fixture::new();
    let read = fixture.effects(UiInput::Key(UiKey::PasteClipboard));
    let [Effect::ReadClipboard { request_id }] = read.as_slice() else {
        panic!("expected clipboard read");
    };
    fixture.input(UiInput::Key(UiKey::Character('n')));

    assert!(
        fixture
            .app
            .complete_clipboard_read(
                *request_id,
                Err(FailureCode::ClipboardFailed),
                &mut fixture.ids,
                &fixture.clock,
            )
            .is_empty()
    );
    assert_eq!(
        fixture
            .app
            .editor_snapshot()
            .expect("materialized editor")
            .content,
        "n"
    );
    assert!(
        fixture
            .app
            .status_text()
            .is_some_and(|status| { status.contains("clipboard unavailable") })
    );
}

#[test]
fn delayed_durable_editor_clipboard_result_cannot_cross_into_board() {
    let mut fixture = Fixture::new();
    fixture.paste("existing draft");
    let read = fixture.effects(UiInput::Key(UiKey::PasteClipboard));
    let [Effect::ReadClipboard { request_id }] = read.as_slice() else {
        panic!("expected clipboard read");
    };
    fixture.input(UiInput::Key(UiKey::Escape));

    let effects = fixture.app.complete_clipboard_read(
        *request_id,
        Ok("stale editor paste".to_owned()),
        &mut fixture.ids,
        &fixture.clock,
    );

    assert!(effects.is_empty());
    assert_eq!(fixture.app.interaction_mode(), InteractionMode::Board);
    let thoughts = fixture.app.state.board.live_thoughts();
    assert_eq!(thoughts.len(), 1);
    assert_eq!(thoughts[0].content, "existing draft");
}

#[test]
fn materialized_compose_clipboard_read_cannot_cross_edit_exit_and_reentry() {
    let mut fixture = Fixture::new();
    let read = fixture.effects(UiInput::Key(UiKey::PasteClipboard));
    let [Effect::ReadClipboard { request_id }] = read.as_slice() else {
        panic!("expected clipboard read");
    };
    fixture.input(UiInput::Key(UiKey::Character('n')));
    fixture.input(UiInput::Key(UiKey::Escape));
    fixture.input(UiInput::Key(UiKey::Enter));

    let effects = fixture.app.complete_clipboard_read(
        *request_id,
        Ok("stale paste".to_owned()),
        &mut fixture.ids,
        &fixture.clock,
    );

    assert!(effects.is_empty());
    assert_eq!(
        fixture
            .app
            .editor_snapshot()
            .expect("re-entered editor")
            .content,
        "n"
    );
}

#[test]
fn materialized_clipboard_image_path_is_one_undoable_paste() {
    let mut fixture = Fixture::new();
    let read = fixture.effects(UiInput::Key(UiKey::PasteClipboard));
    let [Effect::ReadClipboard { request_id }] = read.as_slice() else {
        panic!("expected clipboard read");
    };
    let path = "/private/proqi/attachments/clipboard-req_06g30t8fudrq55fdkjqr6mpe44.png";
    let effects = fixture.app.complete_clipboard_read(
        *request_id,
        Ok(path.to_owned()),
        &mut fixture.ids,
        &fixture.clock,
    );
    assert!(matches!(
        effects.as_slice(),
        [Effect::CommitBoardOperation(_)]
    ));
    assert_eq!(fixture.app.state.board.live_thoughts()[0].content, path);

    fixture.input(UiInput::Key(UiKey::Escape));
    fixture.input(UiInput::Key(UiKey::Undo));
    assert!(fixture.app.state.board.live_thoughts().is_empty());
}

#[test]
fn materialized_path_in_edit_mode_inserts_at_the_cursor() {
    let mut fixture = Fixture::new();
    fixture.paste("attach: ");
    let read = fixture.effects(UiInput::Key(UiKey::PasteClipboard));
    let [Effect::ReadClipboard { request_id }] = read.as_slice() else {
        panic!("expected clipboard read");
    };
    let path = "/private/proqi/Grüße 第一.png";
    let effects = fixture.app.complete_clipboard_read(
        *request_id,
        Ok(path.to_owned()),
        &mut fixture.ids,
        &fixture.clock,
    );
    assert!(matches!(effects.as_slice(), [Effect::CommitRevision(_)]));
    assert_eq!(
        fixture.app.editor_snapshot().expect("editor").content,
        format!("attach: {path}")
    );
}
