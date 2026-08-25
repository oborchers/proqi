//! Temporary update-barrier state around ordinary durable session resume.

use crate::{
    application::DurabilityState,
    domain::{RequestId, StableVersion, Timestamp},
};

use super::BoardApp;

pub(super) struct UpdateBarrier {
    operation_id: RequestId,
    deadline: Timestamp,
}

impl BoardApp {
    pub(crate) fn begin_update_barrier(
        &mut self,
        operation_id: RequestId,
        deadline: Timestamp,
    ) -> bool {
        if self
            .update_barrier
            .as_ref()
            .is_some_and(|barrier| barrier.operation_id != operation_id)
        {
            return false;
        }
        self.update_barrier = Some(UpdateBarrier {
            operation_id,
            deadline,
        });
        self.set_warning("Ready for Proqi update. Waiting for all sessions.");
        true
    }

    pub(crate) fn release_update_barrier(&mut self, operation_id: RequestId) -> bool {
        if self
            .update_barrier
            .as_ref()
            .is_none_or(|barrier| barrier.operation_id != operation_id)
        {
            return false;
        }
        self.update_barrier = None;
        self.set_info("Update cancelled. Session is ready.");
        true
    }

    pub(crate) fn expire_update_barrier(&mut self, now: Timestamp) -> bool {
        if self
            .update_barrier
            .as_ref()
            .is_none_or(|barrier| now < barrier.deadline)
        {
            return false;
        }
        self.update_barrier = None;
        self.set_warning("Update coordinator timed out. Session is ready.");
        true
    }

    pub(crate) fn request_update_restart(
        &mut self,
        operation_id: RequestId,
        installed: StableVersion,
    ) -> bool {
        if self
            .update_barrier
            .as_ref()
            .is_none_or(|barrier| barrier.operation_id != operation_id)
        {
            return false;
        }
        self.update_restart = Some(installed);
        self.quit = true;
        true
    }

    pub(crate) fn update_restart(&self) -> Option<&StableVersion> {
        self.update_restart.as_ref()
    }

    #[cfg(test)]
    pub(crate) fn update_barrier_operation(&self) -> Option<RequestId> {
        self.update_barrier
            .as_ref()
            .map(|barrier| barrier.operation_id)
    }

    pub(crate) fn update_preflight_ready(&self) -> bool {
        self.pending_edit.is_none()
            && matches!(self.state.durability, DurabilityState::Durable { .. })
    }

    pub(crate) fn update_preflight_failed(&self) -> bool {
        matches!(self.state.durability, DurabilityState::Failed { .. })
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        adapters::{editor::RopeEditorFactory, memory::FakeIdGenerator},
        application::AppState,
        domain::{Session, SessionBoard, Timestamp},
        ports::environment::IdGenerator as _,
    };

    use super::BoardApp;

    #[test]
    fn barrier_blocks_competing_attempts_and_expires_safely() {
        let mut ids = FakeIdGenerator::new(1_800_000_000_000);
        let session = Session::new(
            ids.session_id(),
            std::env::temp_dir(),
            Timestamp::from_millis(1),
        )
        .expect("session");
        let board = SessionBoard::new(session, Vec::new()).expect("board");
        let mut app = BoardApp::new(AppState::new(board), RopeEditorFactory);
        let operation = ids.request_id();
        assert!(app.begin_update_barrier(operation, Timestamp::from_millis(10)));
        assert!(!app.begin_update_barrier(ids.request_id(), Timestamp::from_millis(11)));
        assert!(!app.expire_update_barrier(Timestamp::from_millis(9)));
        assert!(app.expire_update_barrier(Timestamp::from_millis(10)));
        assert_eq!(app.update_barrier_operation(), None);
    }
}
