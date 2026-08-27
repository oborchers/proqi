//! Redacted adjacent-agent target identity projection.

use sha2::{Digest as _, Sha256};

use crate::ports::agent::AgentTarget;

pub(super) fn target_fingerprint(target: &AgentTarget) -> [u8; 32] {
    let mut hasher = Sha256::new();
    let identity = target.identity();
    for field in [
        identity.provider.as_str(),
        identity.workspace_id.as_str(),
        identity.tab_id.as_str(),
        identity.source_pane_id.as_str(),
        identity.target_pane_id.as_str(),
        identity.direction.as_str(),
        identity.agent_kind.as_str(),
    ] {
        hasher.update(field.as_bytes());
        hasher.update([0]);
    }
    match identity.agent_session.as_id() {
        Some(session_id) => {
            hasher.update([1]);
            hasher.update(session_id.as_bytes());
        }
        None => hasher.update([0]),
    }
    hasher.finalize().into()
}
