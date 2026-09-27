//! Closed, content-free evidence at input supervision transitions only.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum InputReaderStage {
    Starting,
    Poll,
    Read,
    Delivery,
    Stopped,
}

impl InputReaderStage {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Starting => "starting",
            Self::Poll => "poll",
            Self::Read => "read",
            Self::Delivery => "delivery",
            Self::Stopped => "stopped",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum InputObservation {
    SupervisorGap,
    ConfirmedStall,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct InputStallEvidence {
    pub(crate) reader_stage: Option<InputReaderStage>,
    pub(crate) reader_stage_elapsed_ms: Option<u64>,
    pub(crate) last_completed_stage: Option<InputReaderStage>,
    pub(crate) last_completed_gap_ms: Option<u64>,
    pub(crate) lease_gap_ms: u64,
    pub(crate) observer_gap_ms: u64,
}

pub(crate) fn record_input_observation(outcome: InputObservation, evidence: InputStallEvidence) {
    let (event, reason, outcome) = match outcome {
        InputObservation::SupervisorGap => ("input_lease_reset", "supervisor_gap", "lease_renewed"),
        InputObservation::ConfirmedStall => ("input_stall", "reader_unresponsive", "confirmed"),
    };
    tracing::warn!(
        event,
        reason,
        outcome,
        reader_stage = evidence.reader_stage.map(InputReaderStage::as_str),
        reader_stage_elapsed_ms = evidence.reader_stage_elapsed_ms,
        last_completed_stage = evidence.last_completed_stage.map(InputReaderStage::as_str),
        last_completed_gap_ms = evidence.last_completed_gap_ms,
        lease_gap_ms = evidence.lease_gap_ms,
        observer_gap_ms = evidence.observer_gap_ms,
    );
}

#[cfg(test)]
mod tests {
    use super::{InputObservation, InputReaderStage, InputStallEvidence, record_input_observation};

    #[test]
    fn reader_stages_are_closed_content_free_codes() {
        assert_eq!(
            [
                InputReaderStage::Starting,
                InputReaderStage::Poll,
                InputReaderStage::Read,
                InputReaderStage::Delivery,
                InputReaderStage::Stopped,
            ]
            .map(InputReaderStage::as_str),
            ["starting", "poll", "read", "delivery", "stopped"]
        );
    }

    #[test]
    fn emitted_evidence_contains_only_reviewed_codes_and_numeric_gaps() {
        let output = tempfile::NamedTempFile::new().expect("diagnostic test sink");
        let subscriber = tracing_subscriber::fmt()
            .json()
            .flatten_event(true)
            .without_time()
            .with_target(false)
            .with_writer(output.reopen().expect("diagnostic writer"))
            .finish();
        let evidence = InputStallEvidence {
            reader_stage: Some(InputReaderStage::Read),
            reader_stage_elapsed_ms: Some(500),
            last_completed_stage: Some(InputReaderStage::Poll),
            last_completed_gap_ms: Some(501),
            lease_gap_ms: 550,
            observer_gap_ms: 50,
        };
        tracing::subscriber::with_default(subscriber, || {
            record_input_observation(InputObservation::ConfirmedStall, evidence);
        });
        let actual: serde_json::Value =
            serde_json::from_slice(&std::fs::read(output.path()).expect("diagnostic bytes"))
                .expect("diagnostic JSON");
        assert_eq!(
            actual,
            serde_json::json!({
                "level": "WARN", "event": "input_stall", "reason": "reader_unresponsive",
                "outcome": "confirmed", "reader_stage": "read", "reader_stage_elapsed_ms": 500,
                "last_completed_stage": "poll", "last_completed_gap_ms": 501,
                "lease_gap_ms": 550, "observer_gap_ms": 50,
            })
        );
    }
}
