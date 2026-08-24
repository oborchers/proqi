//! Best-effort display-only identity for a Herdr-managed pane.

use std::time::{Duration, Instant};

use super::{ExternalLane, TerminalError};

const PANE_METADATA_TTL: Duration = Duration::from_secs(15);
const PANE_METADATA_REFRESH: Duration = Duration::from_secs(10);

pub(super) struct PaneHeartbeat {
    pane_id: String,
    sequence: u64,
    next_refresh: Instant,
}

impl PaneHeartbeat {
    pub(super) fn from_pane_id(pane_id: String) -> Option<Self> {
        if pane_id.is_empty() {
            return None;
        }
        Some(Self {
            pane_id,
            sequence: 1,
            next_refresh: Instant::now() + PANE_METADATA_REFRESH,
        })
    }

    pub(super) fn publish(&mut self, external: &ExternalLane) -> Result<(), TerminalError> {
        external.publish_pane(&self.pane_id, self.sequence, PANE_METADATA_TTL)?;
        self.next_refresh = Instant::now() + PANE_METADATA_REFRESH;
        Ok(())
    }

    pub(super) fn refresh_if_due(&mut self, external: &ExternalLane) -> Result<(), TerminalError> {
        if Instant::now() < self.next_refresh {
            return Ok(());
        }
        self.sequence = self.sequence.saturating_add(1);
        self.publish(external)
    }

    pub(super) fn clear(&mut self, external: &ExternalLane) -> Result<(), TerminalError> {
        self.sequence = self.sequence.saturating_add(1);
        external.clear_pane(&self.pane_id, self.sequence)
    }
}

#[cfg(test)]
mod tests {
    use super::PaneHeartbeat;

    #[test]
    fn verified_pane_identity_creates_a_heartbeat_without_environment_state() {
        let heartbeat = PaneHeartbeat::from_pane_id("wA:pC".to_owned());

        assert_eq!(
            heartbeat.map(|value| value.pane_id),
            Some("wA:pC".to_owned())
        );
    }

    #[test]
    fn empty_pane_identity_is_rejected() {
        assert!(PaneHeartbeat::from_pane_id(String::new()).is_none());
    }
}
