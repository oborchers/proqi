use super::*;

#[test]
fn expanded_inaccessible_attachment_keeps_a_plain_warning_snapshot() {
    let mut fixture = Fixture::new();
    let path = "/private/TemporaryItems/missing.png";
    let effects = fixture.effects(UiInput::PasteAnnotated(PastePayload::annotated(
        path.to_owned(),
        vec![ContentAnnotation {
            start: 0,
            end: path.len(),
            kind: ContentAnnotationKind::Attachment {
                image: true,
                display_name: "missing.png".to_owned(),
            },
        }],
    )));
    let batch = effects
        .iter()
        .find_map(|effect| match effect {
            Effect::CheckAttachments(batch) => Some(batch.clone()),
            _ => None,
        })
        .expect("insertion check");
    fixture
        .app
        .complete_attachment_checks(AttachmentCheckBatchResult {
            id: batch.id,
            purpose: batch.purpose,
            results: batch
                .checks
                .into_iter()
                .map(|key| AttachmentCheckResult {
                    key,
                    result: Err(AttachmentAccessFailure::Missing),
                })
                .collect(),
        });
    fixture.input(UiInput::Key(UiKey::Move {
        movement: proqi::ports::editor::CursorMovement::GraphemeBack,
        extend_selection: false,
    }));
    fixture.input(UiInput::Key(UiKey::Enter));

    insta::with_settings!({ snapshot_path => "." }, {
        insta::assert_snapshot!(snapshot(&mut fixture, 60, 8, ThemePreference::Dark));
    });
}
