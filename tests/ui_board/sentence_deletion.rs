//! Sentence deletion UI ownership, discovery, folds, and persistent history.

use super::*;
use proqi::domain::TextPosition;

#[test]
fn default_sentence_chord_commits_one_immediate_editor_revision() {
    let mut fixture = Fixture::new();
    fixture.paste("First sentence. Second sentence.");
    fixture.input(UiInput::Key(UiKey::Move {
        movement: CursorMovement::DocumentStart,
        extend_selection: false,
    }));

    let effects = fixture.effects(UiInput::Key(UiKey::PrimaryCharacter('U')));
    let [Effect::CommitRevision(revision)] = effects.as_slice() else {
        panic!("expected one durable sentence revision: {effects:?}");
    };
    assert_eq!(revision.before_content, "First sentence. Second sentence.");
    assert_eq!(revision.after_content, "Second sentence.");
    assert_eq!(
        fixture.app.editor_snapshot().expect("editor").content,
        "Second sentence."
    );
}

#[test]
fn primary_u_still_deletes_only_the_current_logical_line() {
    let mut fixture = Fixture::new();
    fixture.paste("First sentence.\nSecond sentence.");
    fixture.input(UiInput::Key(UiKey::Move {
        movement: CursorMovement::DocumentStart,
        extend_selection: false,
    }));
    fixture.input(UiInput::Key(UiKey::DeleteLogicalLine));
    assert_eq!(
        fixture.app.editor_snapshot().expect("editor").content,
        "Second sentence."
    );
}

#[test]
fn configured_primary_shift_suffix_discovers_the_same_action() {
    let mut settings = UiSettings::default();
    settings.keybindings.delete_sentence = 'G';
    let mut fixture = Fixture::with_settings(settings);
    fixture.paste("One. Two.");
    fixture.input(UiInput::Key(UiKey::Move {
        movement: CursorMovement::DocumentStart,
        extend_selection: false,
    }));

    fixture.input(UiInput::Key(UiKey::PrimaryCharacter('G')));
    assert_eq!(
        fixture.app.editor_snapshot().expect("editor").content,
        "Two."
    );
}

#[test]
fn palette_restores_a_selection_and_deletes_every_touched_sentence() {
    let mut fixture = Fixture::new();
    fixture.paste("One. Two. Three. Four.");
    fixture.input(UiInput::Key(UiKey::Move {
        movement: CursorMovement::DocumentStart,
        extend_selection: false,
    }));
    for _ in 0..8 {
        fixture.input(UiInput::Key(UiKey::Move {
            movement: CursorMovement::GraphemeForward,
            extend_selection: false,
        }));
    }
    for _ in 0..7 {
        fixture.input(UiInput::Key(UiKey::Move {
            movement: CursorMovement::GraphemeForward,
            extend_selection: true,
        }));
    }
    fixture.input(UiInput::Key(UiKey::Escape));
    fixture.input(UiInput::Key(UiKey::Character(':')));
    for character in "delete sentence".chars() {
        fixture.input(UiInput::Key(UiKey::Character(character)));
    }
    let (_, entries, _) = fixture.app.palette_view().expect("palette");
    assert_eq!(entries, vec!["Delete sentence"]);

    fixture.input(UiInput::Key(UiKey::Enter));
    assert_eq!(
        fixture.app.editor_snapshot().expect("editor").content,
        "One. Four."
    );
}

#[test]
fn sentence_palette_fallback_is_mouse_operable() {
    let mut fixture = Fixture::new();
    fixture.paste("One. Two.");
    fixture.input(UiInput::Key(UiKey::Escape));
    fixture.input(UiInput::Key(UiKey::Character(':')));
    for character in "delete sentence".chars() {
        fixture.input(UiInput::Key(UiKey::Character(character)));
    }
    let _terminal = draw(&mut fixture, 50, 10);
    let item = fixture
        .app
        .prepare_frame(Rect::new(0, 0, 50, 10))
        .overlay
        .expect("command overlay")
        .items[0];
    fixture.pointer(item.x, item.y, PointerKind::Down(PointerButton::Left));
    assert_eq!(
        fixture.app.editor_snapshot().expect("editor").content,
        "One."
    );
}

#[test]
fn sentence_deletion_rebases_unrelated_fold_annotations_exactly() {
    let content = "Remove me. File /tmp/image.png remains. Last.";
    let path = "/tmp/image.png";
    let start = content.find(path).expect("path");
    let annotation = ContentAnnotation {
        start,
        end: start + path.len(),
        kind: ContentAnnotationKind::Attachment {
            image: true,
            display_name: "image.png".to_owned(),
        },
    };
    let mut fixture = Fixture::new();
    fixture.input(UiInput::PasteAnnotated(
        PastePayload::annotated(content.to_owned(), vec![annotation])
            .expect("valid attachment annotation"),
    ));
    fixture.input(UiInput::Key(UiKey::Move {
        movement: CursorMovement::DocumentStart,
        extend_selection: false,
    }));
    fixture.input(UiInput::Key(UiKey::PrimaryCharacter('U')));

    let thought = &fixture.app.state.board.live_thoughts()[0];
    assert_eq!(thought.content, "File /tmp/image.png remains. Last.");
    assert_eq!(thought.annotations.len(), 1);
    assert_eq!(thought.annotations[0].start, "File ".len());
    assert_eq!(thought.annotations[0].end, "File ".len() + path.len());
    assert_eq!(
        fixture.app.editor_snapshot().expect("editor").cursor,
        TextPosition::new(0, 0)
    );
}

#[test]
fn sentence_with_a_fold_is_revealed_unchanged_then_deleted_on_repeat() {
    let content = "File /tmp/image.png attached. Keep this.";
    let path = "/tmp/image.png";
    let start = content.find(path).expect("path");
    let mut fixture = Fixture::new();
    fixture.input(UiInput::PasteAnnotated(
        PastePayload::annotated(
            content.to_owned(),
            vec![ContentAnnotation {
                start,
                end: start + path.len(),
                kind: ContentAnnotationKind::Attachment {
                    image: true,
                    display_name: "image.png".to_owned(),
                },
            }],
        )
        .expect("valid attachment annotation"),
    ));
    fixture.input(UiInput::Key(UiKey::Move {
        movement: CursorMovement::DocumentStart,
        extend_selection: false,
    }));
    let before = fixture.app.editor_snapshot().expect("editor");
    assert!(
        fixture
            .effects(UiInput::Key(UiKey::PrimaryCharacter('U')))
            .is_empty()
    );
    assert_eq!(fixture.app.editor_snapshot().expect("editor"), before);
    assert_eq!(
        fixture.app.status_text(),
        Some("Sentence contains folded content. Review it, then delete again.")
    );
    assert!(text(draw(&mut fixture, 60, 8).backend().buffer()).contains(path));
    assert_eq!(fixture.app.state.board.live_thoughts()[0].content, content);

    let effects = fixture.effects(UiInput::Key(UiKey::PrimaryCharacter('U')));
    assert!(matches!(effects.as_slice(), [Effect::CommitRevision(_)]));

    let thought = &fixture.app.state.board.live_thoughts()[0];
    assert_eq!(thought.content, "Keep this.");
    assert!(thought.annotations.is_empty());
}

#[test]
fn every_intersecting_fold_is_revealed_while_an_unrelated_fold_stays_collapsed() {
    let content = "Use /tmp/a.png and /tmp/b.png now. Keep /tmp/c.png.";
    let annotations = ["/tmp/a.png", "/tmp/b.png", "/tmp/c.png"]
        .into_iter()
        .map(|path| {
            let start = content.find(path).expect("path");
            ContentAnnotation {
                start,
                end: start + path.len(),
                kind: ContentAnnotationKind::Attachment {
                    image: true,
                    display_name: path.trim_start_matches("/tmp/").to_owned(),
                },
            }
        })
        .collect::<Vec<_>>();
    let mut fixture = Fixture::with_annotated_thought(content, annotations);
    fixture.input(UiInput::Key(UiKey::Enter));
    fixture.input(UiInput::Key(UiKey::Move {
        movement: CursorMovement::DocumentStart,
        extend_selection: false,
    }));

    fixture.input(UiInput::Key(UiKey::PrimaryCharacter('U')));
    let rendered = text(draw(&mut fixture, 80, 10).backend().buffer());
    assert!(rendered.contains("/tmp/a.png"));
    assert!(rendered.contains("/tmp/b.png"));
    assert!(rendered.contains("[Image"));
    assert!(!rendered.contains("/tmp/c.png"));

    fixture.input(UiInput::Key(UiKey::PrimaryCharacter('U')));
    let thought = &fixture.app.state.board.live_thoughts()[0];
    assert_eq!(thought.content, "Keep /tmp/c.png.");
    assert_eq!(thought.annotations.len(), 1);
    assert_eq!(thought.annotations[0].start, "Keep ".len());
}

#[test]
fn semantic_emphasis_is_not_a_fold_and_rebases_or_dissolves_normally() {
    let content = "Press Cmd+U now. Keep Cmd+Z.";
    let annotations = ["Cmd+U", "Cmd+Z"]
        .into_iter()
        .map(|token| {
            let start = content.find(token).expect("semantic token");
            serde_json::from_value(serde_json::json!({
                "start": start,
                "end": start + token.len(),
                "kind": { "kind": "shortcut_emphasis" }
            }))
            .expect("valid semantic annotation")
        })
        .collect::<Vec<ContentAnnotation>>();
    let mut fixture = Fixture::with_annotated_thought(content, annotations);
    fixture.input(UiInput::Key(UiKey::Enter));
    fixture.input(UiInput::Key(UiKey::Move {
        movement: CursorMovement::DocumentStart,
        extend_selection: false,
    }));

    let effects = fixture.effects(UiInput::Key(UiKey::PrimaryCharacter('U')));
    assert!(matches!(effects.as_slice(), [Effect::CommitRevision(_)]));
    assert_eq!(fixture.app.status_text(), None);
    let thought = &fixture.app.state.board.live_thoughts()[0];
    assert_eq!(thought.content, "Keep Cmd+Z.");
    assert_eq!(thought.annotations.len(), 1);
    assert_eq!(
        (thought.annotations[0].start, thought.annotations[0].end),
        ("Keep ".len(), "Keep Cmd+Z".len())
    );
}

#[test]
fn persistent_undo_and_redo_restore_the_sentence_cursor_exactly() {
    let mut fixture = Fixture::new();
    fixture.paste("First sentence. Second sentence.");
    fixture.input(UiInput::Key(UiKey::Move {
        movement: CursorMovement::DocumentStart,
        extend_selection: false,
    }));
    for _ in 0..6 {
        fixture.input(UiInput::Key(UiKey::Move {
            movement: CursorMovement::GraphemeForward,
            extend_selection: false,
        }));
    }
    let before = fixture
        .app
        .editor_snapshot()
        .expect("before deletion")
        .cursor;

    fixture.input(UiInput::Key(UiKey::PrimaryCharacter('U')));
    let after = fixture
        .app
        .editor_snapshot()
        .expect("after deletion")
        .cursor;
    assert_eq!(after, TextPosition::new(0, 0));

    fixture.input(UiInput::Key(UiKey::Undo));
    let undone = fixture.app.editor_snapshot().expect("after undo");
    assert_eq!(undone.content, "First sentence. Second sentence.");
    assert_eq!(undone.cursor, before);

    fixture.input(UiInput::Key(UiKey::Redo));
    let redone = fixture.app.editor_snapshot().expect("after redo");
    assert_eq!(redone.content, "Second sentence.");
    assert_eq!(redone.cursor, after);
}

#[test]
fn board_and_search_keep_primary_shift_u_outside_sentence_dispatch() {
    let mut fixture = Fixture::new();
    fixture.paste("Keep this sentence.");
    fixture.input(UiInput::Key(UiKey::Escape));
    assert!(
        fixture
            .effects(UiInput::Key(UiKey::PrimaryCharacter('U')))
            .is_empty()
    );
    assert_eq!(
        fixture.app.state.board.live_thoughts()[0].content,
        "Keep this sentence."
    );

    fixture.input(UiInput::Key(UiKey::Character('/')));
    fixture.input(UiInput::Key(UiKey::PrimaryCharacter('U')));
    assert_eq!(fixture.app.search_view().expect("search").0, "");
    assert_eq!(fixture.app.state.board.live_thoughts().len(), 1);
}
