//! Portable snapshots for recovering an in-memory board after storage failure.

use crate::{domain::Timestamp, ports::recovery::RecoveryDocument};

use super::{AppState, DurabilityState};

/// Capture current reducer state without consulting SQLite.
#[must_use]
pub fn capture_recovery(state: &AppState, exported_at: Timestamp) -> RecoveryDocument {
    RecoveryDocument {
        format_version: 1,
        exported_at,
        session: state.board.session.clone(),
        thoughts: state.board.thoughts().to_vec(),
        pending_sequences: state.pending_sequences.iter().copied().collect(),
        failed_sequence: match state.durability {
            DurabilityState::Failed { failed, .. } => Some(failed),
            DurabilityState::Durable { .. } | DurabilityState::Pending { .. } => None,
        },
    }
}
