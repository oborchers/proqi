//! Authority required immediately before adopting an external installation.

use thiserror::Error;

use crate::domain::{InstanceId, RequestId, SessionId, StableVersion};
use crate::ports::runtime::{Lease, RuntimeError};

use super::UpdateError;

/// Typed failure to prove the final external-adoption boundary.
#[derive(Debug, Error)]
pub enum ExternalUpgradeAuthorityError {
    /// The real schema lease or runtime filesystem boundary refused ownership.
    #[error(transparent)]
    Runtime(#[from] RuntimeError),
    /// The active installation could not be revalidated.
    #[error(transparent)]
    Update(#[from] UpdateError),
}

/// Proves that no schema user remains and the active installation is still the one verified.
///
/// The returned lease retains exclusive schema ownership across the atomic cache transition.
/// Implementations must acquire that ownership before revalidating installation identity, so a
/// failed check cannot leave a window in which an older writer re-enters before adoption.
pub trait ExternalUpgradeAdoptionAuthority {
    /// Establish the active executable identity before any live owner is prepared.
    ///
    /// # Errors
    ///
    /// Returns a typed installation-verification failure without changing cached state.
    fn establish(&mut self) -> Result<(), ExternalUpgradeAuthorityError> {
        self.revalidate_installation()
    }

    /// Revalidate the active installation immediately before a post-replacement cache transition.
    ///
    /// # Errors
    ///
    /// Returns a typed installation-verification failure without changing cached state.
    fn revalidate_installation(&mut self) -> Result<(), ExternalUpgradeAuthorityError>;

    /// Acquire one bounded proof immediately before external cache adoption.
    ///
    /// # Errors
    ///
    /// Returns a typed schema-coordination or installation-verification failure without changing
    /// cached installation state.
    fn acquire(&mut self) -> Result<Box<dyn Lease>, ExternalUpgradeAuthorityError>;
}

/// Replaces the current Unix process after all terminal-owned resources are released.
pub trait ProcessReplacer {
    /// Replace this process with the verified executable resuming one session.
    ///
    /// # Errors
    ///
    /// Returns only when process replacement is unsupported or `exec` fails.
    fn replace(
        &self,
        executable: &std::path::Path,
        session_id: SessionId,
        state_root: Option<&std::path::Path>,
        operation_id: RequestId,
        previous_instance_id: InstanceId,
        target_version: &StableVersion,
    ) -> Result<(), UpdateError>;
}
