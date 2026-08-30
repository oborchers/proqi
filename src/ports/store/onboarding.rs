//! Versioned durable first-run practice-board requests.

use crate::domain::SessionBoard;

/// Durable onboarding version understood by this binary.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct OnboardingVersion(u32);

impl OnboardingVersion {
    /// The once-only six-thought practice board.
    pub const PRACTICE_BOARD: Self = Self(1);

    /// Stable integer stored at the SQLite boundary.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Candidate session and ordinary thoughts for one onboarding version.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FirstRunBoard {
    version: OnboardingVersion,
    board: SessionBoard,
}

impl FirstRunBoard {
    /// Construct an already domain-validated candidate board.
    #[must_use]
    pub const fn new(version: OnboardingVersion, board: SessionBoard) -> Self {
        Self { version, board }
    }

    /// Version this candidate would durably complete.
    #[must_use]
    pub const fn version(&self) -> OnboardingVersion {
        self.version
    }

    /// Candidate session and thoughts.
    #[must_use]
    pub const fn board(&self) -> &SessionBoard {
        &self.board
    }
}

/// Durable result of atomically creating one eligible interactive session.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FirstRunOutcome {
    /// This transaction claimed eligibility and inserted the practice thoughts.
    Seeded,
    /// An earlier transaction completed this version, so the session is empty.
    AlreadyCompleted,
}
