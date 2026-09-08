use super::*;
use proqi::ports::attachment_accessibility::AttachmentAvailability;

#[test]
fn cloud_attachment_states_have_plain_warning_snapshots() {
    let mut fixture = Fixture::new();
    let image = "/private/TemporaryItems/cloud.png";
    let file = "/private/TemporaryItems/downloading.txt";
    let content = format!("{image}\n{file}");
    let file_start = image.len() + 1;
    let effects = fixture.effects(UiInput::PasteAnnotated(
        PastePayload::annotated(
            content,
            vec![
                ContentAnnotation {
                    start: 0,
                    end: image.len(),
                    kind: ContentAnnotationKind::Attachment {
                        ordinal: Some(1_u64.try_into().expect("fixture ordinal")),
                        image: true,
                        display_name: "cloud.png".to_owned(),
                    },
                },
                ContentAnnotation {
                    start: file_start,
                    end: file_start + file.len(),
                    kind: ContentAnnotationKind::Attachment {
                        ordinal: Some(1_u64.try_into().expect("fixture ordinal")),
                        image: false,
                        display_name: "downloading.txt".to_owned(),
                    },
                },
            ],
        )
        .expect("valid attachment payload"),
    ));
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
                .enumerate()
                .map(|(index, key)| AttachmentCheckResult {
                    key,
                    result: Ok(if index == 0 {
                        AttachmentAvailability::InCloud
                    } else {
                        AttachmentAvailability::Downloading
                    }),
                })
                .collect(),
        });
    fixture.input(crate::key_input(UiKey::Escape));

    insta::with_settings!({ snapshot_path => "." }, {
        insta::assert_snapshot!(snapshot(&mut fixture, 60, 8, ThemePreference::Dark));
    });
}

#[test]
fn expanded_inaccessible_attachment_keeps_a_plain_warning_snapshot() {
    let mut fixture = Fixture::new();
    let path = "/private/TemporaryItems/missing.png";
    let effects = fixture.effects(UiInput::PasteAnnotated(
        PastePayload::annotated(
            path.to_owned(),
            vec![ContentAnnotation {
                start: 0,
                end: path.len(),
                kind: ContentAnnotationKind::Attachment {
                    ordinal: Some(1_u64.try_into().expect("fixture ordinal")),
                    image: true,
                    display_name: "missing.png".to_owned(),
                },
            }],
        )
        .expect("valid attachment payload"),
    ));
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
    fixture.input(crate::key_input(UiKey::Move {
        movement: proqi::ports::editor::CursorMovement::GraphemeBack,
        extend_selection: false,
    }));
    fixture.input(crate::key_input(UiKey::Enter));

    insta::with_settings!({ snapshot_path => "." }, {
        assert_platform_snapshot!(snapshot(&mut fixture, 60, 8, ThemePreference::Dark));
    });
}
