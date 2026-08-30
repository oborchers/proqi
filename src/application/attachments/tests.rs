use std::time::Duration;

use crate::{
    application::Effect,
    domain::{
        ContentAnnotation, ContentAnnotationKind, Session, SessionBoard, Thought, ThoughtPosition,
        Timestamp,
    },
    ports::{
        attachment_accessibility::{
            AttachmentAccessFailure, AttachmentCheckBatch, AttachmentCheckBatchResult,
            AttachmentCheckPurpose, AttachmentCheckResult,
        },
        environment::IdGenerator as _,
    },
};

use super::AttachmentAccessibilityState;
use crate::application::test_support::TestIds;

#[test]
fn startup_prioritizes_focus_and_bounds_large_board_batches() {
    let (board, ids) = board_with_attachments(40);
    let focused = ids.get(23).copied();
    let mut state = AttachmentAccessibilityState::default();
    let first = one_batch(state.start(&board, focused, Duration::ZERO));
    assert_eq!(first.checks.len(), 16);
    assert_eq!(
        first.checks[0].thought_id,
        focused.expect("focused thought")
    );

    let next = one_batch(state.complete(accessible(first)).0);
    assert_eq!(next.checks.len(), 16);
    let final_batch = one_batch(state.complete(accessible(next)).0);
    assert_eq!(final_batch.checks.len(), 8);
    assert!(state.complete(accessible(final_batch)).0.is_empty());
}

#[test]
fn stale_mutation_result_is_ignored_and_only_changed_thought_is_invalidated() {
    let (mut board, ids) = board_with_attachments(2);
    let mut state = AttachmentAccessibilityState::default();
    let first = one_batch(state.start(&board, Some(ids[0]), Duration::ZERO));

    let changed = board.thought_mut(ids[0]).expect("first thought");
    changed.content.push_str(" changed");
    let mutation_effects = state.reconcile(&board);
    assert!(
        mutation_effects.is_empty(),
        "existing batch remains bounded"
    );
    let follow_up = one_batch(state.complete(accessible(first)).0);
    assert_eq!(follow_up.checks.len(), 1);
    assert_eq!(follow_up.checks[0].thought_id, ids[0]);
    assert!(!state.inaccessible(ids[1], 0));
}

#[test]
fn inaccessible_health_is_binary_and_manual_refresh_recovers_it() {
    let (board, ids) = board_with_attachments(1);
    let mut state = AttachmentAccessibilityState::default();
    let first = one_batch(state.start(&board, Some(ids[0]), Duration::ZERO));
    let missing = failed(first, AttachmentAccessFailure::Missing);
    assert!(state.complete(missing).0.is_empty());
    assert!(state.inaccessible(ids[0], 0));

    let refresh = one_batch(state.refresh_all(&board, Some(ids[0])));
    assert!(state.complete(accessible(refresh)).0.is_empty());
    assert!(!state.inaccessible(ids[0], 0));
}

#[test]
fn preflight_is_fresh_aggregate_and_outranks_background_continuation() {
    let (board, ids) = board_with_attachments(35);
    let mut generator = TestIds::new(1_725_000_000_100);
    let submission_id = generator.submission_id();
    let mut state = AttachmentAccessibilityState::default();
    let background = one_batch(state.start(&board, Some(ids[0]), Duration::ZERO));
    let keys = state.keys_for(&ids).expect("keys");
    let (effects, immediate) = state.begin_preflight(submission_id, keys);
    assert!(effects.is_empty());
    assert_eq!(immediate, None);

    let preflight = one_batch(state.complete(accessible(background)).0);
    assert_eq!(
        preflight.purpose,
        AttachmentCheckPurpose::SubmissionPreflight(submission_id)
    );
    assert_eq!(preflight.checks.len(), 32);
    let second = one_batch(
        state
            .complete(failed(preflight, AttachmentAccessFailure::PermissionDenied))
            .0,
    );
    let (effects, outcome) = state.complete(accessible(second));
    assert_eq!(outcome.expect("aggregate").inaccessible, 32);
    assert!(matches!(
        effects.as_slice(),
        [Effect::CheckAttachments(batch)] if batch.purpose == AttachmentCheckPurpose::Background
    ));
}

#[test]
fn inactivity_fallback_fires_once_and_repeated_focus_does_no_work_when_fresh() {
    let (board, ids) = board_with_attachments(1);
    let mut state = AttachmentAccessibilityState::default();
    let first = one_batch(state.start(&board, Some(ids[0]), Duration::ZERO));
    assert!(state.complete(accessible(first)).0.is_empty());
    assert!(state.prioritize_focus(ids[0]).is_empty());
    assert!(
        state
            .note_deliberate_interaction(&board, Some(ids[0]), Duration::from_secs(299))
            .is_empty()
    );
    let refresh =
        state.note_deliberate_interaction(&board, Some(ids[0]), Duration::from_secs(10 * 60));
    assert!(matches!(refresh.as_slice(), [Effect::CheckAttachments(_)]));
    assert!(
        state
            .note_deliberate_interaction(&board, Some(ids[0]), Duration::from_secs(10 * 60 + 1),)
            .is_empty()
    );
}

#[test]
fn focus_transition_reprioritizes_unknown_work_once() {
    let (board, ids) = board_with_attachments(40);
    let mut state = AttachmentAccessibilityState::default();
    let first = one_batch(state.start(&board, Some(ids[0]), Duration::ZERO));
    assert!(state.prioritize_focus(ids[39]).is_empty());
    assert!(state.prioritize_focus(ids[39]).is_empty());

    let focused = one_batch(state.complete(accessible(first)).0);
    assert_eq!(focused.checks[0].thought_id, ids[39]);
    let next = one_batch(state.complete(accessible(focused)).0);
    let mut remaining = next;
    loop {
        let (effects, outcome) = state.complete(accessible(remaining));
        assert_eq!(outcome, None);
        let Some(Effect::CheckAttachments(batch)) = effects.first() else {
            break;
        };
        remaining = batch.clone();
    }
    assert!(state.prioritize_focus(ids[39]).is_empty());
}

#[test]
fn every_typed_failure_has_the_same_binary_health() {
    for failure in [
        AttachmentAccessFailure::Missing,
        AttachmentAccessFailure::PermissionDenied,
        AttachmentAccessFailure::Unmounted,
        AttachmentAccessFailure::Unreadable,
        AttachmentAccessFailure::Io,
        AttachmentAccessFailure::TimedOut,
        AttachmentAccessFailure::Cancelled,
    ] {
        let (board, ids) = board_with_attachments(1);
        let mut state = AttachmentAccessibilityState::default();
        let batch = one_batch(state.start(&board, Some(ids[0]), Duration::ZERO));
        assert!(state.complete(failed(batch, failure)).0.is_empty());
        assert!(state.inaccessible(ids[0], 0), "failure: {failure:?}");
    }
}

fn board_with_attachments(count: usize) -> (SessionBoard, Vec<crate::domain::ThoughtId>) {
    let mut ids = TestIds::new(1_725_000_000_000);
    let now = Timestamp::from_millis(1_725_000_000_000);
    let session = Session::new(ids.session_id(), "/tmp/proqi".into(), now).expect("session");
    let mut thought_ids = Vec::new();
    let thoughts = (0..count)
        .map(|index| {
            let id = ids.thought_id();
            thought_ids.push(id);
            let path = format!("/tmp/Grüße-{index}.png");
            let mut thought = Thought::new(
                id,
                session.id,
                path.clone(),
                ThoughtPosition::new(u32::try_from(index).expect("position")),
                now,
            );
            thought
                .set_annotations(vec![ContentAnnotation {
                    start: 0,
                    end: path.len(),
                    kind: ContentAnnotationKind::Attachment {
                        image: true,
                        display_name: format!("Grüße-{index}.png"),
                    },
                }])
                .expect("annotation");
            thought
        })
        .collect();
    (
        SessionBoard::new(session, thoughts).expect("board"),
        thought_ids,
    )
}

fn one_batch(effects: Vec<Effect>) -> AttachmentCheckBatch {
    let mut effects = effects.into_iter();
    let Some(Effect::CheckAttachments(batch)) = effects.next() else {
        panic!("one attachment batch expected");
    };
    assert!(effects.next().is_none());
    batch
}

fn accessible(batch: AttachmentCheckBatch) -> AttachmentCheckBatchResult {
    completion(batch, |_| Ok(()))
}

fn failed(
    batch: AttachmentCheckBatch,
    failure: AttachmentAccessFailure,
) -> AttachmentCheckBatchResult {
    completion(batch, |_| Err(failure))
}

fn completion(
    batch: AttachmentCheckBatch,
    result: impl Fn(
        &crate::ports::attachment_accessibility::AttachmentCheckKey,
    ) -> Result<(), AttachmentAccessFailure>,
) -> AttachmentCheckBatchResult {
    AttachmentCheckBatchResult {
        id: batch.id,
        purpose: batch.purpose,
        results: batch
            .checks
            .into_iter()
            .map(|key| AttachmentCheckResult {
                result: result(&key),
                key,
            })
            .collect(),
    }
}
