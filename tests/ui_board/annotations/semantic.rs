use super::*;

#[test]
fn public_annotated_paste_rejects_deserialized_shortcut_emphasis() {
    let annotation: ContentAnnotation = serde_json::from_value(serde_json::json!({
        "start": 6,
        "end": 11,
        "kind": { "kind": "shortcut_emphasis" }
    }))
    .expect("structurally valid durable annotation");

    assert_eq!(
        PastePayload::annotated("Press Enter".to_owned(), vec![annotation]),
        Err(proqi::domain::DomainError::InvalidContentAnnotation)
    );
}

#[test]
fn ordinary_editor_changes_rebase_outside_and_dissolve_inside_shortcut_ranges() {
    let annotation: ContentAnnotation = serde_json::from_value(serde_json::json!({
        "start": 3,
        "end": 8,
        "kind": { "kind": "shortcut_emphasis" }
    }))
    .expect("structurally valid durable fixture");
    let mut fixture = Fixture::with_annotated_thought("AA Enter ZZ", vec![annotation]);
    fixture.input(UiInput::Key(UiKey::Enter));
    fixture.input(UiInput::Key(UiKey::Move {
        movement: CursorMovement::DocumentStart,
        extend_selection: false,
    }));
    fixture.input(UiInput::Key(UiKey::Character('!')));
    let effects = fixture
        .app
        .flush_pending_edit(&mut fixture.ids, &fixture.clock);
    let [Effect::CommitRevision(revision)] = effects.as_slice() else {
        panic!("one prefix revision");
    };
    assert_eq!(revision.after_content, "!AA Enter ZZ");
    assert_eq!(revision.after_annotations.len(), 1);
    assert_eq!(
        (
            revision.after_annotations[0].start,
            revision.after_annotations[0].end
        ),
        (4, 9)
    );

    fixture.input(UiInput::Key(UiKey::Move {
        movement: CursorMovement::DocumentStart,
        extend_selection: false,
    }));
    for _ in 0..5 {
        fixture.input(UiInput::Key(UiKey::Move {
            movement: CursorMovement::GraphemeForward,
            extend_selection: false,
        }));
    }
    fixture.input(UiInput::Key(UiKey::Character('x')));
    let effects = fixture
        .app
        .flush_pending_edit(&mut fixture.ids, &fixture.clock);
    let [Effect::CommitRevision(revision)] = effects.as_slice() else {
        panic!("one intersecting revision");
    };
    assert_eq!(revision.after_content, "!AA Exnter ZZ");
    assert!(revision.after_annotations.is_empty());
}
