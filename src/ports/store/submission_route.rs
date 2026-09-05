//! Content-redacted durable route encoding for submission recovery.

use crate::{
    domain::Direction,
    ports::agent::{SubmissionRoute, SubmissionRouteKind},
};

/// Current content-redacted journal encoding for submission routes.
pub const SUBMISSION_ROUTE_VERSION: u32 = 1;

/// Versioned content-redacted delivery route retained by the submission journal.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SubmissionJournalRoute {
    version: u32,
    kind: SubmissionRouteKind,
    adjacent_direction: Option<Direction>,
}

impl SubmissionJournalRoute {
    /// Construct the current adjacent-route journal encoding.
    #[must_use]
    pub const fn adjacent(direction: Direction) -> Self {
        Self {
            version: SUBMISSION_ROUTE_VERSION,
            kind: SubmissionRouteKind::AdjacentPane,
            adjacent_direction: Some(direction),
        }
    }

    /// Construct the current global Herdr-route journal encoding.
    #[must_use]
    pub const fn herdr_agent() -> Self {
        Self {
            version: SUBMISSION_ROUTE_VERSION,
            kind: SubmissionRouteKind::HerdrAgent,
            adjacent_direction: None,
        }
    }

    /// Project a verified route without persisting topology identity.
    #[must_use]
    pub const fn from_route(route: &SubmissionRoute) -> Self {
        match route {
            SubmissionRoute::AdjacentPane { direction, .. } => Self::adjacent(*direction),
            SubmissionRoute::HerdrAgent(_) => Self::herdr_agent(),
        }
    }

    /// Decode one exact legacy direction as the original adjacent route.
    #[must_use]
    pub const fn legacy_adjacent(direction: Direction) -> Self {
        Self {
            version: 0,
            kind: SubmissionRouteKind::AdjacentPane,
            adjacent_direction: Some(direction),
        }
    }

    /// Return the durable route encoding version.
    #[must_use]
    pub const fn version(self) -> u32 {
        self.version
    }

    /// Return the closed durable route kind.
    #[must_use]
    pub const fn kind(self) -> SubmissionRouteKind {
        self.kind
    }

    /// Return direction only for an adjacent route.
    #[must_use]
    pub const fn adjacent_direction(self) -> Option<Direction> {
        self.adjacent_direction
    }
}
