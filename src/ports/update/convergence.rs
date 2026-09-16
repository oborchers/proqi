//! Installation-wide update admission and exact cache transition types.

/// Installation-wide lock purpose.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UpdateLockKind {
    /// Own the one permitted background network refresh.
    Refresh,
    /// Own the one actionable update prompt.
    Prompt,
    /// Own the one approved installer invocation.
    Installer,
    /// Exclude schema entrants while one installed cohort becomes quiescent.
    Convergence,
}

/// RAII installation lock released on drop or process exit.
pub trait UpdateLease: Send {}

/// Result of an exact compare-and-set update-cache transition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExternalCacheTransition {
    /// This caller durably applied the requested transition.
    Applied,
    /// The exact requested state was already durable.
    AlreadyApplied,
    /// Cache state changed and no write was made.
    Conflict,
}
