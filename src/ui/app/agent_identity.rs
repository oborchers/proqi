//! Redacted adjacent-agent target identity projection.

use sha2::{Digest as _, Sha256};

use crate::ports::agent::AgentTarget;

pub(super) fn target_fingerprint(target: &AgentTarget) -> [u8; 32] {
    let mut hasher = Sha256::new();
    let identity = target.identity();
    hasher.update(crate::ports::store::SUBMISSION_ROUTE_VERSION.to_be_bytes());
    for field in [
        identity.provider.as_str(),
        identity.route_kind.as_str(),
        identity.workspace_id.as_str(),
        identity.tab_id.as_str(),
        identity.target_pane_id.as_str(),
        identity.agent_kind.as_str(),
    ] {
        hasher.update(field.as_bytes());
        hasher.update([0]);
    }
    if let Some(source_pane_id) = identity.source_pane_id.as_deref() {
        hasher.update(source_pane_id.as_bytes());
    }
    hasher.update([0]);
    if let Some(direction) = identity.direction {
        hasher.update(direction.as_str().as_bytes());
    }
    hasher.update([0]);
    match identity.agent_session.as_id() {
        Some(session_id) => {
            hasher.update([1]);
            hasher.update(session_id.as_bytes());
        }
        None => hasher.update([0]),
    }
    hasher.finalize().into()
}
