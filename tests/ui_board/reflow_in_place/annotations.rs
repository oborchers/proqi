//! In-place transformation of annotation envelopes and protected semantic ranges.

use super::{Fixture, draw, key_input, reflow, text};
use proqi::{
    domain::{ContentAnnotation, ContentAnnotationKind},
    ui::{UiInput, UiKey},
};
use unicode_segmentation::UnicodeSegmentation as _;

#[test]
fn semantic_annotations_rebase_exactly_beside_a_transformable_envelope() {
    for edit in [false, true] {
        let mut fixture = Fixture::new();
        let envelope = format!("  {}\nwrap  ", "界".repeat(1200));
        let source = format!(
            "  before\nprose\n\n{envelope}\n\n/tmp/image.png\n\nagent location\n\nEnter\n\nafter\nprose  "
        );
        fixture.paste(&source);
        fixture.input(key_input(UiKey::Escape));
        let thought_id = fixture.app.active_thought_id().expect("focus");
        let annotations = semantic_annotations(&source, &envelope);
        fixture
            .app
            .state
            .board
            .thought_mut(thought_id)
            .expect("thought")
            .set_annotations(annotations.clone())
            .expect("valid ranges");
        if edit {
            fixture.input(key_input(UiKey::Enter));
        }
        reflow(&mut fixture);
        let after = fixture
            .app
            .state
            .board
            .thought(thought_id)
            .expect("thought");
        assert!(after.content.starts_with("before prose\n\n"));
        assert!(after.content.ends_with("\n\nafter prose"));
        assert_eq!(after.annotations.len(), 4);
        for (before, current) in annotations.iter().zip(&after.annotations).skip(1) {
            assert_eq!(
                &source[before.start..before.end],
                &after.content[current.start..current.end]
            );
            assert_eq!(before.kind, current.kind);
        }
        let fold = &after.annotations[0];
        assert_eq!(
            &after.content[fold.start..fold.end],
            format!("{} wrap", "界".repeat(1200))
        );
        assert!(matches!(
            fold.kind,
            ContentAnnotationKind::LargePaste {
                lines: 1,
                graphemes: 1205
            }
        ));
        proqi::domain::validate_annotations(&after.content, &after.annotations)
            .expect("valid transformed metadata");
        assert!(reflow(&mut fixture).is_empty());
        fixture.input(key_input(UiKey::Undo));
        let restored = fixture
            .app
            .state
            .board
            .thought(thought_id)
            .expect("restored");
        assert_eq!(restored.content, source);
        assert_eq!(restored.annotations, annotations);
    }
}

#[test]
fn expanded_envelope_stays_expanded_after_reflow() {
    let mut fixture = Fixture::new();
    let source = format!("{}\ncontinuation", "word ".repeat(300));
    fixture.paste(&source);
    fixture.input(key_input(UiKey::Move {
        movement: proqi::ports::editor::CursorMovement::GraphemeBack,
        extend_selection: false,
    }));
    fixture.input(key_input(UiKey::Enter));
    assert!(!text(draw(&mut fixture, 50, 10).backend().buffer()).contains("[Pasted text"));
    reflow(&mut fixture);
    assert!(!text(draw(&mut fixture, 50, 10).backend().buffer()).contains("[Pasted text"));
    assert_eq!(
        fixture.app.editor_snapshot().expect("editor").content,
        format!("{}continuation", "word ".repeat(300))
    );
}

#[test]
fn failed_transform_keeps_the_exact_large_control_payload() {
    let mut fixture = Fixture::new();
    let source = format!("{}\u{7}\nline", "a".repeat(1200));
    fixture.paste(&source);
    let before = fixture.app.state.board.live_thoughts()[0].clone();
    assert!(reflow(&mut fixture).is_empty());
    assert_eq!(fixture.app.state.board.live_thoughts()[0], &before);
    assert_eq!(
        fixture.app.status_text(),
        Some("could not reflow; thought kept unchanged")
    );
    fixture.input(UiInput::Paste(" ordinary exact\n paste".to_owned()));
    assert!(
        fixture
            .app
            .editor_snapshot()
            .expect("editor")
            .content
            .ends_with(" ordinary exact\n paste")
    );
}

fn semantic_annotations(source: &str, envelope: &str) -> Vec<ContentAnnotation> {
    let mut annotations = Vec::new();
    for (value, kind) in [
        (
            envelope,
            ContentAnnotationKind::LargePaste {
                lines: 2,
                graphemes: envelope.graphemes(true).count(),
            },
        ),
        (
            "/tmp/image.png",
            ContentAnnotationKind::Attachment {
                image: true,
                display_name: "image.png".to_owned(),
            },
        ),
        (
            "agent location",
            ContentAnnotationKind::InvocationReference {
                display_name: "@agent".to_owned(),
            },
        ),
    ] {
        let start = source.find(value).expect("range");
        annotations.push(ContentAnnotation {
            start,
            end: start + value.len(),
            kind,
        });
    }
    let start = source.find("Enter").expect("shortcut");
    annotations.push(
        serde_json::from_value(
            serde_json::json!({"start":start,"end":start+5,"kind":{"kind":"shortcut_emphasis"}}),
        )
        .expect("stored instructional annotation"),
    );
    annotations
}
