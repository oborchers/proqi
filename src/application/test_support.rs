//! Deterministic inner-layer test values.

use uuid::Uuid;

use crate::{
    domain::{
        InstanceId, OperationId, RequestId, RevisionId, SessionId, SubmissionId, ThoughtId,
        Timestamp,
    },
    ports::environment::{Clock, IdGenerator},
};

pub(super) struct TestClock(pub(super) Timestamp);

impl Clock for TestClock {
    fn now(&self) -> Timestamp {
        self.0
    }
}

pub(super) struct TestIds {
    timestamp_ms: u64,
    counter: u64,
}

impl TestIds {
    pub(super) const fn new(timestamp_ms: u64) -> Self {
        Self {
            timestamp_ms,
            counter: 0,
        }
    }

    fn uuid(&mut self) -> Uuid {
        let mut bytes = [0_u8; 16];
        let timestamp = self.timestamp_ms.to_be_bytes();
        bytes[..6].copy_from_slice(&timestamp[2..]);
        bytes[6] = 0x70;
        let counter = self.counter.to_be_bytes();
        bytes[8..].copy_from_slice(&counter);
        bytes[8] = (bytes[8] & 0x3f) | 0x80;
        self.counter = self.counter.wrapping_add(1);
        Uuid::from_bytes(bytes)
    }
}

macro_rules! generate {
    ($self:ident, $type:ty) => {
        loop {
            if let Ok(id) = <$type>::from_uuid($self.uuid()) {
                break id;
            }
        }
    };
}

impl IdGenerator for TestIds {
    fn session_id(&mut self) -> SessionId {
        generate!(self, SessionId)
    }
    fn thought_id(&mut self) -> ThoughtId {
        generate!(self, ThoughtId)
    }
    fn revision_id(&mut self) -> RevisionId {
        generate!(self, RevisionId)
    }
    fn operation_id(&mut self) -> OperationId {
        generate!(self, OperationId)
    }
    fn instance_id(&mut self) -> InstanceId {
        generate!(self, InstanceId)
    }
    fn request_id(&mut self) -> RequestId {
        generate!(self, RequestId)
    }
    fn submission_id(&mut self) -> SubmissionId {
        generate!(self, SubmissionId)
    }
}
