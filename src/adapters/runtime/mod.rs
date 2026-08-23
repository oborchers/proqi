//! Process coordination, leases, paths, and local control transport.

use std::time::{SystemTime, UNIX_EPOCH};

use uuid::Uuid;

use crate::{
    domain::{
        InstanceId, OperationId, RequestId, RevisionId, SessionId, SubmissionId, ThoughtId,
        Timestamp,
    },
    ports::environment::{Clock, IdGenerator},
};

/// Operating-system UTC clock.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Timestamp {
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_millis());
        Timestamp::from_millis(i64::try_from(millis).unwrap_or(i64::MAX))
    }
}

/// System `UUIDv7` generator.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemIdGenerator;

macro_rules! system_id {
    ($type:ty) => {
        <$type>::from_uuid(Uuid::now_v7()).expect("uuid crate generated UUIDv7")
    };
}

impl IdGenerator for SystemIdGenerator {
    fn session_id(&mut self) -> SessionId {
        system_id!(SessionId)
    }

    fn thought_id(&mut self) -> ThoughtId {
        system_id!(ThoughtId)
    }

    fn revision_id(&mut self) -> RevisionId {
        system_id!(RevisionId)
    }

    fn operation_id(&mut self) -> OperationId {
        system_id!(OperationId)
    }

    fn instance_id(&mut self) -> InstanceId {
        system_id!(InstanceId)
    }

    fn request_id(&mut self) -> RequestId {
        system_id!(RequestId)
    }

    fn submission_id(&mut self) -> SubmissionId {
        system_id!(SubmissionId)
    }
}
