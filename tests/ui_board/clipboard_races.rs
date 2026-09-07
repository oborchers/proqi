//! Delayed clipboard completions that must stay bound to their source editor.

use super::Fixture;
use proqi::{
    application::Effect,
    domain::{ContentAnnotation, ContentAnnotationKind},
    ports::editor::CursorMovement,
    ui::{PastePayload, UiInput, UiKey},
};

#[test]
fn delayed_editor_cut_cannot_delete_an_identical_annotated_neighbor() {
    let content = "/tmp/repeated.png";
    let annotation = ContentAnnotation {
        start: 0,
        end: content.len(),
        kind: ContentAnnotationKind::Attachment {
            ordinal: Some(1_u64.try_into().expect("fixture ordinal")),
            image: true,
            display_name: "repeated.png".to_owned(),
        },
    };
    let mut fixture = Fixture::with_annotated_thought(content, vec![annotation.clone()]);
    fixture.input(UiInput::PasteAnnotated(
        PastePayload::annotated(content.to_owned(), vec![annotation.clone()])
            .expect("second annotated thought"),
    ));
    fixture.input(crate::key_input(UiKey::Escape));
    fixture.input(crate::key_input(UiKey::Move {
        movement: CursorMovement::VisualUp,
        extend_selection: false,
    }));
    fixture.input(crate::key_input(UiKey::Enter));
    fixture.input(crate::key_input(UiKey::SelectAll));
    let cut = fixture.effects(crate::key_input(UiKey::Cut));
    let [Effect::WriteClipboard { request_id, .. }] = cut.as_slice() else {
        panic!("expected selection write");
    };

    fixture.input(crate::key_input(UiKey::Escape));
    fixture.input(crate::key_input(UiKey::Move {
        movement: CursorMovement::VisualDown,
        extend_selection: false,
    }));
    fixture.input(crate::key_input(UiKey::Enter));
    fixture.input(crate::key_input(UiKey::SelectAll));
    let completion =
        fixture
            .app
            .complete_clipboard_write(*request_id, Ok(()), &mut fixture.ids, &fixture.clock);

    assert!(completion.is_empty());
    assert_eq!(fixture.app.state.board.live_thoughts().len(), 2);
    for (index, thought) in fixture
        .app
        .state
        .board
        .live_thoughts()
        .into_iter()
        .enumerate()
    {
        let mut expected = annotation.clone();
        if let ContentAnnotationKind::Attachment { ordinal, .. } = &mut expected.kind {
            *ordinal = Some(
                u64::try_from(index + 1)
                    .expect("index")
                    .try_into()
                    .expect("ordinal"),
            );
        }
        assert_eq!(thought.content, content);
        assert_eq!(thought.annotations, vec![expected]);
    }
    assert_eq!(
        fixture.app.status_text(),
        Some("selection changed before clipboard confirmation")
    );
}
