use super::*;
use proqi::{
    application::InteractionMode,
    domain::{BoardOperationKind, ContentAnnotation, ContentAnnotationKind, UndoScope},
};

fn query_palette(fixture: &mut Fixture, query: &str) {
    fixture.input(UiInput::Key(UiKey::Character(':')));
    for character in query.chars() {
        fixture.input(UiInput::Key(UiKey::Character(character)));
    }
}

fn board_operation(effects: &[Effect]) -> &proqi::domain::BoardOperation {
    let [Effect::CommitBoardOperation(operation)] = effects else {
        panic!("expected one board operation: {effects:?}");
    };
    operation
}

#[test]
fn keyboard_palette_splits_at_exact_unicode_cursor_across_resize_and_undoes_once() {
    let mut fixture = Fixture::new();
    fixture.paste("A界\r\nB");
    fixture.input(UiInput::Key(UiKey::Move {
        movement: CursorMovement::DocumentStart,
        extend_selection: false,
    }));
    for _ in 0..2 {
        fixture.input(UiInput::Key(UiKey::Move {
            movement: CursorMovement::GraphemeForward,
            extend_selection: false,
        }));
    }
    fixture.input(UiInput::Key(UiKey::Escape));
    query_palette(&mut fixture, "split thought");
    fixture.input(UiInput::Resize {
        width: 23,
        height: 6,
    });
    let effects = fixture.effects(UiInput::Key(UiKey::Enter));
    assert_eq!(board_operation(&effects).kind, BoardOperationKind::Split);
    let live = fixture.app.state.board.live_thoughts();
    assert_eq!(live[0].content, "A界");
    assert_eq!(live[1].content, "\r\nB");
    let right = live[1].id;
    assert_eq!(
        fixture.app.interaction_mode(),
        InteractionMode::Edit { thought_id: right }
    );
    assert_eq!(
        fixture.app.editor_snapshot().expect("right editor").cursor,
        proqi::domain::TextPosition::new(0, 0)
    );

    let undo = fixture.effects(UiInput::Key(UiKey::Undo));
    assert!(matches!(
        undo.as_slice(),
        [Effect::CommitHistoryMove {
            scope: UndoScope::Board,
            undo: true,
            ..
        }]
    ));
    assert_eq!(
        fixture.app.state.board.live_thoughts()[0].content,
        "A界\r\nB"
    );
    let redo = fixture.effects(UiInput::Key(UiKey::Redo));
    assert!(matches!(
        redo.as_slice(),
        [Effect::CommitHistoryMove {
            scope: UndoScope::Board,
            undo: false,
            ..
        }]
    ));
    assert_eq!(fixture.app.state.board.live_thoughts().len(), 2);
}

#[test]
fn mouse_palette_extracts_a_reverse_selection_and_places_cursor_at_new_end() {
    let mut fixture = Fixture::new();
    let exact = "zero 日本\r\nend";
    fixture.paste(exact);
    fixture.input(UiInput::Key(UiKey::Move {
        movement: CursorMovement::DocumentStart,
        extend_selection: true,
    }));
    fixture.input(UiInput::Key(UiKey::Escape));
    query_palette(&mut fixture, "extract selection");
    let item = fixture
        .app
        .prepare_frame(Rect::new(0, 0, 62, 8))
        .overlay
        .expect("palette")
        .items[0];
    let effects = fixture.effects(UiInput::Pointer(PointerInput {
        column: item.x,
        row: item.y,
        kind: PointerKind::Down(PointerButton::Left),
        extend_selection: false,
    }));
    assert_eq!(board_operation(&effects).kind, BoardOperationKind::Extract);
    let live = fixture.app.state.board.live_thoughts();
    assert_eq!(live[0].content, "");
    assert_eq!(live[1].content, exact);
    assert_eq!(
        fixture
            .app
            .editor_snapshot()
            .expect("extracted editor")
            .cursor,
        proqi::domain::TextPosition::new(1, 3)
    );
}

#[test]
fn split_uses_annotations_rebased_by_the_edit_flushed_on_exit() {
    let mut fixture = Fixture::new();
    fixture.input(UiInput::PasteAnnotated(PastePayload::annotated(
        "abcdef".to_owned(),
        vec![ContentAnnotation {
            start: 0,
            end: 6,
            kind: ContentAnnotationKind::Attachment {
                image: false,
                display_name: "fold.txt".to_owned(),
            },
        }],
    )));
    fixture.input(UiInput::Key(UiKey::Move {
        movement: CursorMovement::GraphemeBack,
        extend_selection: false,
    }));
    fixture.input(UiInput::Key(UiKey::Enter));
    let _terminal = draw(&mut fixture, 50, 8);
    let area = fixture.app.prepare_frame(Rect::new(0, 0, 50, 8)).thoughts[0].text_area;
    fixture.pointer(
        area.x.saturating_add(1),
        area.y,
        PointerKind::Down(PointerButton::Left),
    );
    fixture.input(UiInput::Key(UiKey::Character('X')));
    fixture.input(UiInput::Key(UiKey::Escape));
    query_palette(&mut fixture, "split thought");
    let effects = fixture.effects(UiInput::Key(UiKey::Enter));
    assert_eq!(board_operation(&effects).kind, BoardOperationKind::Split);
    let live = fixture.app.state.board.live_thoughts();
    assert_eq!(live[0].content, "aX");
    assert_eq!(live[1].content, "bcdef");
    assert!(live.iter().all(|thought| thought.annotations.is_empty()));
}

#[test]
fn palette_rejects_annotation_only_staleness_with_actionable_feedback() {
    let mut fixture = Fixture::new();
    fixture.input(UiInput::PasteAnnotated(PastePayload::annotated(
        "folded".to_owned(),
        vec![ContentAnnotation {
            start: 0,
            end: 6,
            kind: ContentAnnotationKind::LargePaste {
                lines: 12,
                graphemes: 6,
            },
        }],
    )));
    fixture.input(UiInput::Key(UiKey::Escape));
    query_palette(&mut fixture, "split thought");
    let thought_id = fixture.app.state.board.live_thoughts()[0].id;
    fixture
        .app
        .state
        .board
        .thought_mut(thought_id)
        .expect("source")
        .annotations
        .clear();

    let effects = fixture.effects(UiInput::Key(UiKey::Enter));
    assert!(effects.is_empty());
    assert_eq!(fixture.app.state.board.live_thoughts().len(), 1);
    assert_eq!(fixture.app.state.board.live_thoughts()[0].content, "folded");
    assert!(text(draw(&mut fixture, 70, 8).backend().buffer()).contains("changed"));
}

#[test]
fn palette_merge_uses_configured_separator_and_rejects_discontiguous_selection() {
    let mut fixture = Fixture::with_settings(UiSettings {
        merge_separator: "\r\n--\r\n".to_owned(),
        ..UiSettings::default()
    });
    for content in ["one", "two", "三"] {
        fixture.paste(content);
        fixture.input(UiInput::Key(UiKey::Escape));
        if content != "三" {
            fixture.input(UiInput::Key(UiKey::Character('n')));
        }
    }
    fixture.input(UiInput::Key(UiKey::Character('a')));
    query_palette(&mut fixture, "merge selected");
    let merged = fixture.effects(UiInput::Key(UiKey::Enter));
    assert_eq!(board_operation(&merged).kind, BoardOperationKind::Merge);
    assert_eq!(
        fixture.app.state.board.live_thoughts()[0].content,
        "one\r\n--\r\ntwo\r\n--\r\n三"
    );
    assert_eq!(fixture.app.interaction_mode(), InteractionMode::Board);

    let mut fixture = Fixture::new();
    for content in ["first", "middle", "last"] {
        fixture.paste(content);
        fixture.input(UiInput::Key(UiKey::Escape));
        if content != "last" {
            fixture.input(UiInput::Key(UiKey::Character('n')));
        }
    }
    fixture.input(UiInput::Key(UiKey::Character(' ')));
    fixture.input(UiInput::Key(UiKey::Character('k')));
    fixture.input(UiInput::Key(UiKey::Character('k')));
    fixture.input(UiInput::Key(UiKey::Character(' ')));
    let before = fixture
        .app
        .state
        .board
        .live_thoughts()
        .iter()
        .map(|thought| thought.content.clone())
        .collect::<Vec<_>>();
    query_palette(&mut fixture, "merge selected");
    let effects = fixture.effects(UiInput::Key(UiKey::Enter));
    assert!(effects.is_empty());
    assert_eq!(
        fixture
            .app
            .state
            .board
            .live_thoughts()
            .iter()
            .map(|thought| thought.content.clone())
            .collect::<Vec<_>>(),
        before
    );
    let terminal = draw(&mut fixture, 70, 10);
    assert!(text(terminal.backend().buffer()).contains("contiguous"));
}
