//! Explicit smart-paste behavior across Board, Compose, Edit, palette, and history.

use proqi::{
    application::{Effect, InteractionMode},
    domain::{ContentAnnotation, ContentAnnotationKind},
    ports::editor::CursorMovement,
    ui::{PastePayload, PointerButton, PointerInput, PointerKind, UiInput, UiKey},
};
use ratatui_core::layout::Rect;

use super::{Fixture, draw, text};

fn request(fixture: &mut Fixture, key: UiKey) -> proqi::domain::RequestId {
    let effects = fixture.effects(UiInput::Key(key));
    let [Effect::ReadClipboard { request_id }] = effects.as_slice() else {
        panic!("expected one clipboard read, got {effects:?}");
    };
    *request_id
}

fn complete(
    fixture: &mut Fixture,
    request_id: proqi::domain::RequestId,
    content: &str,
) -> Vec<Effect> {
    fixture.app.complete_clipboard_read(
        request_id,
        Ok(content.to_owned()),
        &mut fixture.ids,
        &fixture.clock,
    )
}

#[test]
fn ordinary_paste_and_bracketed_paste_remain_byte_exact() {
    let content = "  copied\r\n\r\n\r\n  exactly\t ";
    let mut native = Fixture::new();
    let exact = request(&mut native, UiKey::PasteClipboard);
    complete(&mut native, exact, content);
    assert_eq!(native.app.state.board.live_thoughts()[0].content, content);

    let mut bracketed = Fixture::new();
    bracketed.input(UiInput::Paste(content.to_owned()));
    assert_eq!(
        bracketed.app.state.board.live_thoughts()[0].content,
        content
    );
}

#[test]
fn reflow_paste_is_one_atomic_board_operation_with_persistent_history() {
    let mut fixture = Fixture::new();
    let request_id = request(&mut fixture, UiKey::PasteClipboardReflow);
    let effects = complete(
        &mut fixture,
        request_id,
        "  first  line\nwraps here\n\n\nsecond\tparagraph  ",
    );
    assert!(matches!(
        effects.as_slice(),
        [Effect::CommitBoardOperation(_)]
    ));
    assert_eq!(
        fixture.app.state.board.live_thoughts()[0].content,
        "first line wraps here\n\nsecond paragraph"
    );
    assert_eq!(fixture.app.status_text(), Some("pasted and reflowed"));

    fixture.input(UiInput::Key(UiKey::Escape));
    fixture.input(UiInput::Key(UiKey::Undo));
    assert!(fixture.app.state.board.live_thoughts().is_empty());
    fixture.input(UiInput::Key(UiKey::Escape));
    fixture.input(UiInput::Key(UiKey::Redo));
    assert_eq!(
        fixture.app.state.board.live_thoughts()[0].content,
        "first line wraps here\n\nsecond paragraph"
    );
}

#[test]
fn reflow_replaces_one_editor_selection_and_undo_restores_it() {
    let mut fixture = Fixture::new();
    fixture.paste("keep OLD suffix");
    for _ in 0.." suffix".chars().count() {
        fixture.input(UiInput::Key(UiKey::Move {
            movement: CursorMovement::GraphemeBack,
            extend_selection: false,
        }));
    }
    for _ in 0.."OLD".chars().count() {
        fixture.input(UiInput::Key(UiKey::Move {
            movement: CursorMovement::GraphemeBack,
            extend_selection: true,
        }));
    }
    let request_id = request(&mut fixture, UiKey::PasteClipboardReflow);
    let effects = complete(&mut fixture, request_id, "new\n  words");
    assert!(matches!(effects.as_slice(), [Effect::CommitRevision(_)]));
    assert_eq!(
        fixture.app.editor_snapshot().expect("editor").content,
        "keep new words suffix"
    );
    fixture.input(UiInput::Key(UiKey::Undo));
    assert_eq!(
        fixture.app.editor_snapshot().expect("editor").content,
        "keep OLD suffix"
    );
}

#[test]
fn whitespace_only_reflow_does_not_delete_a_selection_or_create_a_thought() {
    let mut compose = Fixture::new();
    let request_id = request(&mut compose, UiKey::PasteClipboardReflow);
    assert!(complete(&mut compose, request_id, " \n\t\n ").is_empty());
    assert!(compose.app.state.board.live_thoughts().is_empty());
    assert_eq!(
        compose.app.status_text(),
        Some("nothing remained after reflow")
    );

    let mut edit = Fixture::new();
    edit.paste("selected");
    edit.input(UiInput::Key(UiKey::SelectAll));
    let request_id = request(&mut edit, UiKey::PasteClipboardReflow);
    assert!(complete(&mut edit, request_id, "\n\n").is_empty());
    assert_eq!(
        edit.app.editor_snapshot().expect("editor").content,
        "selected"
    );
}

#[test]
fn protected_attachment_payload_pastes_exactly_with_truthful_status() {
    let path = "/tmp/Some File.png";
    let content = format!("{path}\ncaption  remains");
    let payload = PastePayload::annotated(
        content.clone(),
        vec![ContentAnnotation {
            start: 0,
            end: path.len(),
            kind: ContentAnnotationKind::Attachment {
                image: true,
                display_name: "Some File.png".to_owned(),
            },
        }],
    )
    .expect("payload");
    let mut fixture = Fixture::new();
    let request_id = request(&mut fixture, UiKey::PasteClipboardReflow);
    fixture.app.complete_clipboard_read_payload(
        request_id,
        Ok(payload),
        &mut fixture.ids,
        &fixture.clock,
    );
    let thought = &fixture.app.state.board.live_thoughts()[0];
    assert_eq!(thought.content, content);
    assert_eq!(thought.annotations.len(), 1);
    assert_eq!(
        fixture.app.status_text(),
        Some("pasted exactly; nothing to reflow")
    );
}

#[test]
fn a_delayed_reflow_read_never_crosses_owners() {
    let mut fixture = Fixture::new();
    let request_id = request(&mut fixture, UiKey::PasteClipboardReflow);
    fixture.input(UiInput::Key(UiKey::Escape));
    assert_eq!(fixture.app.interaction_mode(), InteractionMode::Board);
    assert!(complete(&mut fixture, request_id, "must\nnot paste").is_empty());
    assert!(fixture.app.state.board.live_thoughts().is_empty());
}

#[test]
fn compose_materialization_preserves_reflow_kind_and_repeated_reads() {
    let mut fixture = Fixture::new();
    let first = request(&mut fixture, UiKey::PasteClipboardReflow);
    fixture.input(UiInput::Key(UiKey::Character('x')));
    let second = request(&mut fixture, UiKey::PasteClipboardReflow);
    complete(&mut fixture, first, " first\npart");
    complete(&mut fixture, second, " second\npart");
    assert_eq!(
        fixture.app.editor_snapshot().expect("editor").content,
        "xfirst partsecond part"
    );
}

#[test]
fn transformed_large_paste_keeps_a_fold_through_narrow_shallow_rendering() {
    let mut fixture = Fixture::new();
    let request_id = request(&mut fixture, UiKey::PasteClipboardReflow);
    complete(
        &mut fixture,
        request_id,
        &format!("{}\n tail", "界".repeat(1_200)),
    );
    assert!(matches!(
        fixture.app.state.board.live_thoughts()[0]
            .annotations
            .as_slice(),
        [ContentAnnotation {
            kind: ContentAnnotationKind::LargePaste { lines: 1, .. },
            ..
        }]
    ));
    let terminal = draw(&mut fixture, 40, 6);
    let rendered = text(terminal.backend().buffer());
    assert!(rendered.contains("[Pasted text"), "{rendered}");
}

#[test]
fn board_fallback_is_reflow_while_text_and_query_owners_keep_their_inputs() {
    for character in ['p', 'P'] {
        let mut compose = Fixture::new();
        assert_eq!(
            compose
                .effects(UiInput::Key(UiKey::Character(character)))
                .len(),
            1
        );
        assert_eq!(
            compose.app.editor_snapshot().expect("compose").content,
            character.to_string()
        );
    }

    for character in ['p', 'P'] {
        let mut board = Fixture::new();
        board.input(UiInput::Key(UiKey::Escape));
        let request_id = request(&mut board, UiKey::Character(character));
        complete(&mut board, request_id, "board\nreflow");
        assert_eq!(
            board.app.state.board.live_thoughts()[0].content,
            "board reflow"
        );
    }

    let mut board = Fixture::new();
    board.input(UiInput::Key(UiKey::Escape));
    board.input(UiInput::Key(UiKey::Character('/')));
    assert!(
        board
            .effects(UiInput::Key(UiKey::PasteClipboardReflow))
            .is_empty()
    );
    board.input(UiInput::Key(UiKey::Escape));
    board.input(UiInput::Key(UiKey::Character(':')));
    assert!(
        board
            .effects(UiInput::Key(UiKey::PasteClipboardReflow))
            .is_empty()
    );
}

#[test]
fn reflow_content_survives_storage_failure_and_uses_the_existing_retry() {
    let mut fixture = Fixture::new();
    let request_id = request(&mut fixture, UiKey::PasteClipboardReflow);
    let effects = complete(&mut fixture, request_id, "save\nthis");
    let sequence = effects
        .first()
        .and_then(Effect::persistence_batch)
        .and_then(|batch| batch.sequence())
        .expect("persistence sequence");
    fixture.app.acknowledge_persistence(sequence, false);
    assert_eq!(
        fixture.app.state.board.live_thoughts()[0].content,
        "save this"
    );
    assert_eq!(
        fixture.effects(UiInput::Key(UiKey::Character('r'))),
        vec![Effect::RetryPersistence { sequence }]
    );
    fixture.app.acknowledge_persistence(sequence, true);
    assert_eq!(
        fixture.app.state.board.live_thoughts()[0].content,
        "save this"
    );
}

#[test]
fn delayed_reflow_completion_never_overwrites_a_storage_failure() {
    let mut fixture = Fixture::new();
    let sequence = fixture.paste("existing");
    fixture.input(UiInput::Key(UiKey::Escape));
    let request_id = request(&mut fixture, UiKey::PasteClipboardReflow);
    fixture.app.acknowledge_persistence(sequence, false);

    assert!(complete(&mut fixture, request_id, "must\nnot paste").is_empty());
    assert_eq!(
        fixture.app.state.board.live_thoughts()[0].content,
        "existing"
    );
    assert!(
        fixture
            .app
            .status_text()
            .is_some_and(|status| status.contains("invalid in the current application state"))
    );
}

#[test]
fn command_palette_reflow_restores_the_editor_selection_handoff() {
    let mut fixture = Fixture::new();
    fixture.paste("replace OLD here");
    for _ in 0.." here".len() {
        fixture.input(UiInput::Key(UiKey::Move {
            movement: CursorMovement::GraphemeBack,
            extend_selection: false,
        }));
    }
    for _ in 0..3 {
        fixture.input(UiInput::Key(UiKey::Move {
            movement: CursorMovement::GraphemeBack,
            extend_selection: true,
        }));
    }
    fixture.input(UiInput::Key(UiKey::Escape));
    fixture.input(UiInput::Key(UiKey::Character(':')));
    for character in "paste and reflow".chars() {
        fixture.input(UiInput::Key(UiKey::Character(character)));
    }
    let request_id = fixture
        .effects(UiInput::Key(UiKey::Enter))
        .into_iter()
        .find_map(|effect| match effect {
            Effect::ReadClipboard { request_id } => Some(request_id),
            _ => None,
        })
        .expect("palette clipboard read");
    complete(&mut fixture, request_id, "new\nwords");
    assert_eq!(
        fixture.app.editor_snapshot().expect("editor").content,
        "replace new words here"
    );
}

#[test]
fn paste_commands_are_discoverable_and_reflow_is_mouse_operable() {
    let mut fixture = Fixture::new();
    fixture.input(UiInput::Key(UiKey::Escape));
    fixture.input(UiInput::Key(UiKey::Character(':')));
    for character in "paste".chars() {
        fixture.input(UiInput::Key(UiKey::Character(character)));
    }
    let (_, entries, _) = fixture.app.palette_view().expect("palette");
    assert!(entries.iter().any(|entry| entry == "Paste exactly"));
    assert!(entries.iter().any(|entry| entry == "Paste and reflow"));

    fixture.input(UiInput::Key(UiKey::Escape));
    fixture.input(UiInput::Key(UiKey::Character(':')));
    for character in "paste and reflow".chars() {
        fixture.input(UiInput::Key(UiKey::Character(character)));
    }
    let item = fixture
        .app
        .prepare_frame(Rect::new(0, 0, 50, 10))
        .overlay
        .expect("overlay")
        .items[0];
    let effects = fixture.effects(UiInput::Pointer(PointerInput {
        column: item.x,
        row: item.y,
        kind: PointerKind::Down(PointerButton::Left),
        extend_selection: false,
    }));
    let [Effect::ReadClipboard { request_id }] = effects.as_slice() else {
        panic!("mouse should request clipboard");
    };
    complete(&mut fixture, *request_id, "mouse\npath");
    assert_eq!(
        fixture.app.state.board.live_thoughts()[0].content,
        "mouse path"
    );
}

#[test]
fn stale_palette_handoff_requests_no_clipboard_read() {
    let mut fixture = Fixture::new();
    fixture.paste("source");
    fixture.input(UiInput::Key(UiKey::SelectAll));
    fixture.input(UiInput::Key(UiKey::Escape));
    fixture.input(UiInput::Key(UiKey::Character(':')));
    for character in "paste and reflow".chars() {
        fixture.input(UiInput::Key(UiKey::Character(character)));
    }
    let thought_id = fixture.app.state.board.live_thoughts()[0].id;
    fixture
        .app
        .state
        .board
        .thought_mut(thought_id)
        .expect("source")
        .content
        .push('!');
    assert!(fixture.effects(UiInput::Key(UiKey::Enter)).is_empty());
    assert_eq!(
        fixture.app.status_text(),
        Some("thought changed before paste was chosen")
    );
}
