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

use super::{AttachmentAccessibilityState, AttachmentRefreshCause};
use crate::application::test_support::TestIds;

mod reconciliation;

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
    changed.content = "/tmp/relinked-during-startup.png".to_owned();
    changed.annotations[0].end = changed.content.len();
    let ContentAnnotationKind::Attachment { display_name, .. } = &mut changed.annotations[0].kind
    else {
        panic!("attachment annotation");
    };
    *display_name = "relinked-during-startup.png".to_owned();
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

    let refresh = one_batch(
        state
            .refresh_all(&board, Some(ids[0]), AttachmentRefreshCause::Manual)
            .0,
    );
    assert!(state.complete(accessible(refresh)).0.is_empty());
    assert!(!state.inaccessible(ids[0], 0));
}

#[test]
fn startup_is_fail_closed_and_rechecks_preserve_the_last_resolved_health() {
    let (board, ids) = board_with_attachments(2);
    let mut state = AttachmentAccessibilityState::default();
    assert!(state.inaccessible(ids[0], 0));

    let startup = one_batch(state.start(&board, Some(ids[0]), Duration::ZERO));
    assert!(state.inaccessible(ids[0], 0));
    assert!(state.inaccessible(ids[1], 0));
    assert!(state.complete(accessible(startup)).0.is_empty());
    assert!(!state.inaccessible(ids[0], 0));
    assert!(!state.inaccessible(ids[1], 0));

    let refresh = one_batch(
        state
            .refresh_all(&board, Some(ids[0]), AttachmentRefreshCause::Manual)
            .0,
    );
    assert!(!state.inaccessible(ids[0], 0));
    assert!(!state.inaccessible(ids[1], 0));
    let (_, _, outcome) = state.complete(accessible(refresh));
    assert_eq!(outcome.expect("manual completion").inaccessible, 0);
    assert!(!state.inaccessible(ids[0], 0));
    assert!(!state.inaccessible(ids[1], 0));
}

#[test]
fn prose_edit_migrates_exact_health_without_repeating_filesystem_work() {
    let (mut board, ids) = board_with_attachments(1);
    let mut state = AttachmentAccessibilityState::default();
    let startup = one_batch(state.start(&board, Some(ids[0]), Duration::ZERO));
    assert!(state.complete(accessible(startup)).0.is_empty());
    let old_key = state.keys_for(&ids).expect("old exact key")[0].clone();

    board
        .thought_mut(ids[0])
        .expect("thought")
        .content
        .push_str(" unrelated prose");
    assert!(state.reconcile(&board).is_empty());
    let new_key = state.keys_for(&ids).expect("new exact key")[0].clone();
    assert_ne!(old_key, new_key);
    assert!(!state.inaccessible(ids[0], 0));
}

#[test]
fn exact_duplicate_match_is_reserved_before_shifted_semantic_migration() {
    let (mut board, ids) = board_with_attachments(1);
    let thought = board.thought_mut(ids[0]).expect("thought");
    let path = thought.content.clone();
    thought.content = format!("{path}\n{path}");
    thought
        .set_annotations(vec![
            ContentAnnotation {
                start: 0,
                end: path.len(),
                kind: ContentAnnotationKind::Attachment {
                    image: true,
                    display_name: "duplicate.png".to_owned(),
                },
            },
            ContentAnnotation {
                start: path.len(),
                end: path.len() + 1,
                kind: ContentAnnotationKind::LargePaste {
                    lines: 1,
                    graphemes: 1,
                },
            },
            ContentAnnotation {
                start: path.len() + 1,
                end: path.len() * 2 + 1,
                kind: ContentAnnotationKind::Attachment {
                    image: true,
                    display_name: "duplicate.png".to_owned(),
                },
            },
        ])
        .expect("duplicate annotations");

    let mut state = AttachmentAccessibilityState::default();
    let startup = one_batch(state.start(&board, Some(ids[0]), Duration::ZERO));
    let mixed = completion(startup, |key| {
        (key.annotation_index != 0)
            .then_some(())
            .ok_or(AttachmentAccessFailure::Missing)
    });
    assert!(state.complete(mixed).0.is_empty());
    assert!(state.inaccessible(ids[0], 0));
    assert!(!state.inaccessible(ids[0], 2));

    board
        .thought_mut(ids[0])
        .expect("thought")
        .annotations
        .remove(1);
    assert!(state.reconcile(&board).is_empty());
    assert!(state.inaccessible(ids[0], 0));
    assert!(!state.inaccessible(ids[0], 1));
}

#[test]
fn source_mutation_restarts_manual_refresh_and_rejects_the_old_generation() {
    let (mut board, ids) = board_with_attachments(1);
    let mut state = AttachmentAccessibilityState::default();
    let startup = one_batch(state.start(&board, Some(ids[0]), Duration::ZERO));
    assert!(state.complete(accessible(startup)).0.is_empty());

    let previous_batch = one_batch(
        state
            .refresh_all(&board, Some(ids[0]), AttachmentRefreshCause::Manual)
            .0,
    );
    let thought = board.thought_mut(ids[0]).expect("thought");
    thought.content = "/tmp/relinked.png".to_owned();
    thought.annotations[0].end = thought.content.len();
    let ContentAnnotationKind::Attachment { display_name, .. } = &mut thought.annotations[0].kind
    else {
        panic!("attachment annotation");
    };
    *display_name = "relinked.png".to_owned();
    assert!(state.reconcile(&board).is_empty());

    let (effects, _, stale_outcome) = state.complete(accessible(previous_batch));
    assert_eq!(stale_outcome, None);
    let current = one_batch(effects);
    assert_eq!(current.checks[0].canonical_path, "/tmp/relinked.png");
    let (_, _, outcome) = state.complete(failed(current, AttachmentAccessFailure::Missing));
    assert_eq!(outcome.expect("current generation").inaccessible, 1);
}

#[test]
fn removing_the_last_attachment_completes_a_superseded_manual_refresh() {
    let (mut board, ids) = board_with_attachments(1);
    let mut state = AttachmentAccessibilityState::default();
    let startup = one_batch(state.start(&board, Some(ids[0]), Duration::ZERO));
    assert!(state.complete(accessible(startup)).0.is_empty());
    let previous_batch = one_batch(
        state
            .refresh_all(&board, Some(ids[0]), AttachmentRefreshCause::Manual)
            .0,
    );
    board
        .thought_mut(ids[0])
        .expect("thought")
        .annotations
        .clear();
    assert!(state.reconcile(&board).is_empty());
    let (effects, _, outcome) = state.complete(accessible(previous_batch));
    assert!(effects.is_empty());
    assert_eq!(outcome.expect("empty current generation").total, 0);
    assert!(!state.manual_refresh_active());
}

#[test]
fn refresh_preserves_inaccessible_health_until_successful_recovery() {
    let (board, ids) = board_with_attachments(1);
    let mut state = AttachmentAccessibilityState::default();
    let startup = one_batch(state.start(&board, Some(ids[0]), Duration::ZERO));
    assert!(
        state
            .complete(failed(startup, AttachmentAccessFailure::Missing))
            .0
            .is_empty()
    );
    assert!(state.inaccessible(ids[0], 0));

    let refresh = one_batch(
        state
            .refresh_all(&board, Some(ids[0]), AttachmentRefreshCause::Manual)
            .0,
    );
    assert!(state.inaccessible(ids[0], 0));
    let (_, _, outcome) = state.complete(accessible(refresh));
    assert_eq!(outcome.expect("recovery").inaccessible, 0);
    assert!(!state.inaccessible(ids[0], 0));
}

#[test]
fn latest_manual_refresh_owns_completion_and_quiet_triggers_coalesce() {
    let (board, ids) = board_with_attachments(1);
    let mut state = AttachmentAccessibilityState::default();
    let startup = one_batch(state.start(&board, Some(ids[0]), Duration::ZERO));
    assert!(state.complete(accessible(startup)).0.is_empty());

    let first = one_batch(
        state
            .refresh_all(&board, Some(ids[0]), AttachmentRefreshCause::Manual)
            .0,
    );
    assert!(
        state
            .refresh_all(&board, Some(ids[0]), AttachmentRefreshCause::Quiet)
            .0
            .is_empty()
    );
    assert!(
        state
            .refresh_all(&board, Some(ids[0]), AttachmentRefreshCause::Manual)
            .0
            .is_empty(),
        "the first bounded batch remains in flight"
    );
    let (effects, _, refresh_outcome) = state.complete(accessible(first));
    assert_eq!(refresh_outcome, None);
    let latest = one_batch(effects);
    let (_, _, completed) = state.complete(failed(latest, AttachmentAccessFailure::Io));
    assert_eq!(completed.expect("latest refresh").inaccessible, 1);
}

#[test]
fn empty_manual_refresh_completes_immediately() {
    let (board, _) = board_with_attachments(0);
    let mut state = AttachmentAccessibilityState::default();
    let (effects, outcome) = state.refresh_all(&board, None, AttachmentRefreshCause::Manual);
    assert!(effects.is_empty());
    assert_eq!(outcome.expect("empty completion").total, 0);
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
    let (effects, outcome, refresh) = state.complete(accessible(second));
    assert_eq!(outcome.expect("aggregate").inaccessible, 32);
    assert_eq!(refresh, None);
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
            .0
            .is_empty()
    );
    let (refresh, refreshed) =
        state.note_deliberate_interaction(&board, Some(ids[0]), Duration::from_secs(10 * 60));
    assert!(refreshed);
    assert!(matches!(refresh.as_slice(), [Effect::CheckAttachments(_)]));
    let refresh = one_batch(refresh);
    assert!(
        state
            .note_deliberate_interaction(&board, Some(ids[0]), Duration::from_secs(10 * 60 + 1),)
            .0
            .is_empty()
    );
    assert!(state.complete(accessible(refresh)).0.is_empty());

    let manual = one_batch(
        state
            .refresh_all(&board, Some(ids[0]), AttachmentRefreshCause::Manual)
            .0,
    );
    let (_, coalesced) =
        state.note_deliberate_interaction(&board, Some(ids[0]), Duration::from_secs(20 * 60));
    assert!(!coalesced);
    let _completion = state.complete(accessible(manual));
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
        let (effects, outcome, refresh) = state.complete(accessible(remaining));
        assert_eq!(outcome, None);
        assert_eq!(refresh, None);
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
