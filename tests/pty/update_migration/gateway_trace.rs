//! Content-free test wrapper, also compiled against the pinned historical source.

use proqi::{
    domain::RequestId,
    ports::{
        runtime::InstanceInfo,
        update::{
            UpdateError, UpdateParticipantGateway, UpdatePrepareReply, UpdatePrepareRequest,
            UpdateQuiesceReply, UpdateQuiesceRequest, UpdateRestartReply, UpdateRestartRequest,
        },
    },
};
use serde::{Deserialize, Serialize};
use std::{
    fs::{File, OpenOptions},
    io::Write as _,
    path::Path,
    time::Instant,
};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(super) enum Stage {
    Prepare,
    Release,
    Quiesce,
    Restart,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(super) enum Outcome {
    Started,
    Ready,
    Blocked,
    Released,
    Quiesced,
    Accepted,
    Refused,
    IdentityMismatch,
    Unsupported,
    InvalidPeer,
    MessageTooLarge,
    ProtocolMismatch,
    Timeout,
    Transport,
    Rejected,
    Coordination,
    State,
    Installation,
    Installer,
    Network,
    InvalidResponse,
    ResponseTooLarge,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Event {
    pub elapsed_ms: u64,
    pub stage: Stage,
    pub outcome: Outcome,
    pub instance: proqi::domain::InstanceId,
    pub session: proqi::domain::SessionId,
    pub pid: u32,
    pub process_present: bool,
    pub endpoint_present: bool,
}

pub(super) struct TracedGateway<G> {
    inner: G,
    output: File,
    started: Instant,
    events: usize,
}

impl<G> TracedGateway<G> {
    pub(super) fn new(inner: G, path: &Path) -> Self {
        Self {
            inner,
            output: OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
                .expect("new gateway trace"),
            started: Instant::now(),
            events: 0,
        }
    }

    fn record(&mut self, owner: &InstanceInfo, stage: Stage, outcome: Outcome) {
        assert!(self.events < 512, "gateway trace event bound");
        self.events += 1;
        let process_present = i32::try_from(owner.pid)
            .ok()
            .and_then(rustix::process::Pid::from_raw)
            .is_some_and(|pid| rustix::process::test_kill_process(pid).is_ok());
        let event = Event {
            elapsed_ms: u64::try_from(self.started.elapsed().as_millis())
                .expect("bounded elapsed time"),
            stage,
            outcome,
            instance: owner.instance_id,
            session: owner.session_id,
            pid: owner.pid,
            process_present,
            endpoint_present: owner
                .control_endpoint
                .as_deref()
                .is_some_and(|path| Path::new(path).exists()),
        };
        serde_json::to_writer(&mut self.output, &event).expect("gateway trace event");
        self.output.write_all(b"\n").expect("gateway trace newline");
        self.output.flush().expect("gateway trace flush");
    }
}

impl<G: UpdateParticipantGateway> UpdateParticipantGateway for TracedGateway<G> {
    fn prepare(
        &mut self,
        owner: &InstanceInfo,
        request: &UpdatePrepareRequest,
    ) -> Result<UpdatePrepareReply, UpdateError> {
        self.record(owner, Stage::Prepare, Outcome::Started);
        let reply = self.inner.prepare(owner, request);
        let outcome = match &reply {
            Ok(UpdatePrepareReply::Ready {
                instance_id,
                session_id,
            }) if *instance_id == owner.instance_id && *session_id == owner.session_id => {
                Outcome::Ready
            }
            Ok(UpdatePrepareReply::Ready { .. }) => Outcome::IdentityMismatch,
            Ok(UpdatePrepareReply::Blocked { .. }) => Outcome::Blocked,
            Err(error) => classify(error),
        };
        self.record(owner, Stage::Prepare, outcome);
        reply
    }

    fn release(&mut self, owner: &InstanceInfo, operation: RequestId) -> Result<(), UpdateError> {
        self.record(owner, Stage::Release, Outcome::Started);
        let reply = self.inner.release(owner, operation);
        self.record(
            owner,
            Stage::Release,
            reply.as_ref().map_or_else(classify, |()| Outcome::Released),
        );
        reply
    }

    fn quiesce(
        &mut self,
        owner: &InstanceInfo,
        request: &UpdateQuiesceRequest,
    ) -> Result<UpdateQuiesceReply, UpdateError> {
        self.record(owner, Stage::Quiesce, Outcome::Started);
        let reply = self.inner.quiesce(owner, request);
        let outcome = match &reply {
            Ok(reply)
                if reply.instance_id == owner.instance_id
                    && reply.session_id == owner.session_id =>
            {
                Outcome::Quiesced
            }
            Ok(_) => Outcome::IdentityMismatch,
            Err(error) => classify(error),
        };
        self.record(owner, Stage::Quiesce, outcome);
        reply
    }

    fn restart(
        &mut self,
        owner: &InstanceInfo,
        request: &UpdateRestartRequest,
    ) -> Result<UpdateRestartReply, UpdateError> {
        self.record(owner, Stage::Restart, Outcome::Started);
        let reply = self.inner.restart(owner, request);
        let outcome = match &reply {
            Ok(reply) if reply.instance_id != owner.instance_id => Outcome::IdentityMismatch,
            Ok(reply) if reply.accepted => Outcome::Accepted,
            Ok(_) => Outcome::Refused,
            Err(error) => classify(error),
        };
        self.record(owner, Stage::Restart, outcome);
        reply
    }
}

// Only closed transport codes cross the report boundary, never error strings.
fn classify(error: &UpdateError) -> Outcome {
    match error {
        UpdateError::Coordination(code) => match code.as_str() {
            "unsupported_update_control" => Outcome::Unsupported,
            "invalid_update_peer" => Outcome::InvalidPeer,
            "update_message_too_large" => Outcome::MessageTooLarge,
            "update_protocol_mismatch" => Outcome::ProtocolMismatch,
            "update_participant_timeout" => Outcome::Timeout,
            "update_transport_failed" => Outcome::Transport,
            "update_participant_rejected" => Outcome::Rejected,
            _ => Outcome::Coordination,
        },
        UpdateError::State(_) => Outcome::State,
        UpdateError::Installation(_) => Outcome::Installation,
        UpdateError::InstallerFailed => Outcome::Installer,
        UpdateError::Network => Outcome::Network,
        UpdateError::InvalidResponse => Outcome::InvalidResponse,
        UpdateError::ResponseTooLarge => Outcome::ResponseTooLarge,
    }
}
