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
fn deleting_a_sentence_that_contains_a_fold_removes_its_annotation() {
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
    fixture.input(UiInput::Key(UiKey::PrimaryCharacter('U')));

    let thought = &fixture.app.state.board.live_thoughts()[0];
    assert_eq!(thought.content, "Keep this.");
    assert!(thought.annotations.is_empty());
}
