//! Trace oracles distinguish restart acknowledgement from later replacement.

use super::gateway_trace::{Event, Outcome, Stage, TracedGateway};
use proqi::{
    adapters::memory::FakeIdGenerator,
    domain::{RequestId, StableVersion, Timestamp},
    ports::{
        environment::IdGenerator as _,
        runtime::InstanceInfo,
        update::{
            UpdateError, UpdateParticipantGateway, UpdatePrepareReply, UpdatePrepareRequest,
            UpdateQuiesceReply, UpdateQuiesceRequest, UpdateRestartReply, UpdateRestartRequest,
        },
    },
};
use std::fs;

struct Replies {
    failure: Option<&'static str>,
}

impl UpdateParticipantGateway for Replies {
    fn prepare(
        &mut self,
        owner: &InstanceInfo,
        _: &UpdatePrepareRequest,
    ) -> Result<UpdatePrepareReply, UpdateError> {
        Ok(UpdatePrepareReply::Ready {
            instance_id: owner.instance_id,
            session_id: owner.session_id,
        })
    }
    fn release(&mut self, _: &InstanceInfo, _: RequestId) -> Result<(), UpdateError> {
        Ok(())
    }
    fn quiesce(
        &mut self,
        owner: &InstanceInfo,
        _: &UpdateQuiesceRequest,
    ) -> Result<UpdateQuiesceReply, UpdateError> {
        Ok(UpdateQuiesceReply {
            instance_id: owner.instance_id,
            session_id: owner.session_id,
        })
    }
    fn restart(
        &mut self,
        owner: &InstanceInfo,
        _: &UpdateRestartRequest,
    ) -> Result<UpdateRestartReply, UpdateError> {
        match self.failure {
            Some("refused") => Ok(UpdateRestartReply {
                instance_id: owner.instance_id,
                accepted: false,
            }),
            Some(code) => Err(UpdateError::Coordination(code.to_owned())),
            None => Ok(UpdateRestartReply {
                instance_id: owner.instance_id,
                accepted: true,
            }),
        }
    }
}

#[test]
fn missing_and_refused_restart_receipts_retain_exact_stage_without_claiming_replacement() {
    let mut ids = FakeIdGenerator::new(1_800_000_000_000);
    let owner = InstanceInfo {
        instance_id: ids.instance_id(),
        session_id: ids.session_id(),
        pid: std::process::id(),
        version: "not exported".to_owned(),
        storage_protocol: 18,
        control_protocol: None,
        control_endpoint: Some("/private/not-exported".to_owned()),
        update: None,
        launch_directory: "/private/not-exported".to_owned(),
        started_at: Timestamp::from_millis(0),
    };
    let request = UpdateRestartRequest {
        operation_id: ids.request_id(),
        installed_version: StableVersion::parse("0.11.0").expect("version"),
    };
    for (failure, expected) in [
        (None, Outcome::Accepted),
        (Some("refused"), Outcome::Refused),
        (Some("update_participant_timeout"), Outcome::Timeout),
        (Some("update_transport_failed"), Outcome::Transport),
        (Some("private error text"), Outcome::Coordination),
    ] {
        let root = tempfile::tempdir().expect("trace fixture");
        let trace = root.path().join("trace.jsonl");
        let mut gateway = TracedGateway::new(Replies { failure }, &trace);
        let actual = gateway.restart(&owner, &request);
        let expected_reply = Replies { failure }.restart(&owner, &request);
        assert_eq!(
            actual, expected_reply,
            "wrapper must not change transport semantics"
        );
        let text = fs::read_to_string(trace).expect("trace");
        assert!(!text.contains("private"));
        assert!(!text.contains("not exported"));
        let events = text
            .lines()
            .map(|line| serde_json::from_str::<Event>(line).expect("typed event"))
            .collect::<Vec<_>>();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].outcome, Outcome::Started);
        assert_eq!(events[1].outcome, expected);
        assert_eq!(events[1].stage, Stage::Restart);
        assert_eq!(events[1].instance, owner.instance_id);
        assert_eq!(events[1].session, owner.session_id);
        assert!(events[1].process_present);
        assert!(events[1].elapsed_ms >= events[0].elapsed_ms);
    }
}
