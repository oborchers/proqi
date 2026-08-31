use super::*;
use proqi::ports::attachment_accessibility::{AttachmentCheckBatchResult, AttachmentCheckResult};

pub(super) fn insert_accessible(fixture: &mut Fixture, payload: PastePayload) {
    let effects = fixture.effects(UiInput::PasteAnnotated(payload));
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
                    result: Ok(()),
                })
                .collect(),
        });
}
