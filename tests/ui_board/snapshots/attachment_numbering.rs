//! Stable multi-digit labels after board movement, with an independent file sequence.

use super::snapshot;
use crate::{Fixture, key_input};
use proqi::{
    domain::{ContentAnnotation, ContentAnnotationKind},
    ports::editor::CursorMovement,
    ui::{PastePayload, ThemePreference, UiInput, UiKey},
};

fn paste(fixture: &mut Fixture, image: bool) {
    let content = "/offline/Grüße asset.png".to_owned();
    let annotation = ContentAnnotation {
        start: 0,
        end: content.len(),
        kind: ContentAnnotationKind::Attachment {
            ordinal: None,
            image,
            display_name: "asset.png".to_owned(),
        },
    };
    fixture.input(UiInput::PasteAnnotated(
        PastePayload::annotated(content, vec![annotation]).expect("payload"),
    ));
    fixture.input(key_input(UiKey::Escape));
}

#[test]
fn shuffled_session_labels_keep_multidigit_image_identity_and_independent_files() {
    let mut fixture = Fixture::new();
    for _ in 0..14 {
        paste(&mut fixture, true);
    }
    fixture.input(key_input(UiKey::PrimaryShiftMove {
        movement: CursorMovement::VisualDown,
    }));
    assert!(matches!(
        fixture.app.state.board.live_thoughts()[0].annotations[0].kind,
        ContentAnnotationKind::Attachment { ordinal: Some(ordinal), .. } if ordinal.get() == 14
    ));
    paste(&mut fixture, false);
    paste(&mut fixture, false);
    for _ in 0..2 {
        fixture.input(key_input(UiKey::Move {
            movement: CursorMovement::VisualUp,
            extend_selection: false,
        }));
    }
    let rendered = snapshot(&mut fixture, 48, 20, ThemePreference::Dark);
    assert!(rendered.contains("Image 13"), "{rendered}");
    assert!(rendered.contains("File 1"));
    assert!(rendered.contains("File 2"));
    insta::assert_snapshot!(rendered);
}

pub(super) fn populated_attachment_fixture() -> Fixture {
    let mut fixture = Fixture::new();
    fixture.input(UiInput::Paste("first prompt".to_owned()));
    fixture.input(crate::key_input(UiKey::Escape));
    fixture.input(UiInput::PasteAnnotated(
        PastePayload::annotated(
            "/private/tmp/Bild (18).png".to_owned(),
            vec![ContentAnnotation {
                start: 0,
                end: "/private/tmp/Bild (18).png".len(),
                kind: ContentAnnotationKind::Attachment {
                    ordinal: Some(1_u64.try_into().expect("fixture ordinal")),
                    image: true,
                    display_name: "Bild (18).png".to_owned(),
                },
            }],
        )
        .expect("valid attachment payload"),
    ));
    fixture.input(crate::key_input(UiKey::Escape));
    fixture
}
