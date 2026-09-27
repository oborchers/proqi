//! Short-lived session ownership for mutations into an inactive destination.

use crate::{
    domain::{InstanceId, SessionId, Timestamp},
    ports::runtime::{RuntimeCoordinator, RuntimeError, RuntimeScan},
};

use super::{FileRuntimeCoordinator, FileSchemaLease, FileSessionLease};

/// A distinct lease identity without interactive control or update capability.
pub(crate) struct TransientSessionCoordinator {
    inner: FileRuntimeCoordinator,
}

impl FileRuntimeCoordinator {
    /// Issue a fresh, scoped owner for an inactive session mutation.
    #[must_use]
    pub(crate) fn transient_session_owner(
        &self,
        instance_id: InstanceId,
        started_at: Timestamp,
    ) -> TransientSessionCoordinator {
        TransientSessionCoordinator {
            inner: Self {
                runtime_dir: self.runtime_dir.clone(),
                instance_id,
                launch_directory: self.launch_directory.clone(),
                started_at,
                version: self.version.clone(),
                update: None,
                schema_lock_policy: self.schema_lock_policy,
            },
        }
    }
}

impl RuntimeCoordinator for TransientSessionCoordinator {
    type SessionLease = FileSessionLease;
    type SharedSchemaLease = FileSchemaLease;
    type ExclusiveSchemaLease = FileSchemaLease;

    fn acquire_session(&self, session_id: SessionId) -> Result<Self::SessionLease, RuntimeError> {
        self.inner.acquire_session_lease(session_id, false)
    }

    fn acquire_schema_shared(&self) -> Result<Self::SharedSchemaLease, RuntimeError> {
        self.inner.acquire_schema_shared()
    }

    fn acquire_schema_exclusive(&self) -> Result<Self::ExclusiveSchemaLease, RuntimeError> {
        self.inner.acquire_schema_exclusive()
    }

    fn scan_runtime(&self) -> Result<RuntimeScan, RuntimeError> {
        self.inner.scan_runtime()
    }
}
