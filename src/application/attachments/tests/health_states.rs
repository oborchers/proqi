use std::time::Duration;

use crate::{
    application::attachments::{AttachmentPresentationState, AttachmentRefreshCause},
    ports::attachment_accessibility::{AttachmentAccessFailure, AttachmentAvailability},
};

use super::{
    AttachmentAccessibilityState, availability, board_with_attachments, failed, one_batch,
};

#[test]
fn refresh_converges_available_to_icloud_to_downloading_to_available() {
    let (board, ids) = board_with_attachments(1);
    let mut state = AttachmentAccessibilityState::default();
    let initial = one_batch(state.start(&board, Some(ids[0]), Duration::ZERO));
    state.complete(availability(initial, AttachmentAvailability::Available));
    assert_eq!(
        state.presentation_state(ids[0], 0),
        AttachmentPresentationState::Available
    );

    for (availability_state, presentation_state) in [
        (
            AttachmentAvailability::InCloud,
            AttachmentPresentationState::InCloud,
        ),
        (
            AttachmentAvailability::Downloading,
            AttachmentPresentationState::Downloading,
        ),
        (
            AttachmentAvailability::Available,
            AttachmentPresentationState::Available,
        ),
    ] {
        let refresh = one_batch(
            state
                .refresh_all(&board, Some(ids[0]), AttachmentRefreshCause::Quiet)
                .0,
        );
        state.complete(availability(refresh, availability_state));
        assert_eq!(state.presentation_state(ids[0], 0), presentation_state);
    }
}

#[test]
fn every_typed_failure_has_generic_inaccessible_health() {
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
        assert_eq!(
            state.presentation_state(ids[0], 0),
            AttachmentPresentationState::Inaccessible,
            "failure: {failure:?}"
        );
    }
}

#[test]
fn manual_refresh_counts_cloud_states_as_unavailable() {
    for cloud_state in [
        AttachmentAvailability::InCloud,
        AttachmentAvailability::Downloading,
    ] {
        let (board, ids) = board_with_attachments(1);
        let mut state = AttachmentAccessibilityState::default();
        let initial = one_batch(state.start(&board, Some(ids[0]), Duration::ZERO));
        state.complete(availability(initial, AttachmentAvailability::Available));
        let refresh = one_batch(
            state
                .refresh_all(&board, Some(ids[0]), AttachmentRefreshCause::Manual)
                .0,
        );
        let (_, _, outcome) = state.complete(availability(refresh, cloud_state));
        assert_eq!(outcome.expect("manual outcome").inaccessible, 1);
    }
}
