//! Terminal-independent accessibility checks for external attachment paths.

use std::{path::Path, time::Duration};

use thiserror::Error;

use crate::domain::{SubmissionId, ThoughtId};

/// Exact transient identity of one annotated attachment revision.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct AttachmentCheckKey {
    /// Thought containing the annotation.
    pub thought_id: ThoughtId,
    /// Annotation position in the thought's sorted annotation vector.
    pub annotation_index: usize,
    /// Inclusive canonical UTF-8 byte offset.
    pub annotation_start: usize,
    /// Exclusive canonical UTF-8 byte offset.
    pub annotation_end: usize,
    /// Whether presentation uses the image label.
    pub image: bool,
    /// Exact presentation metadata participating in cache identity.
    pub display_name: String,
    /// Exact canonical path stored in prompt content.
    pub canonical_path: String,
    /// Digest of the exact canonical content revision.
    pub content_revision: [u8; 32],
}

impl AttachmentCheckKey {
    /// Borrow the exact canonical path without resolving or rewriting it.
    #[must_use]
    pub fn path(&self) -> &Path {
        Path::new(&self.canonical_path)
    }
}

/// Why an adapter could not prove that an attachment is accessible.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum AttachmentAccessFailure {
    /// No filesystem entry currently resolves at the path.
    #[error("missing")]
    Missing,
    /// The current process does not have permission to read the entry.
    #[error("permission denied")]
    PermissionDenied,
    /// The path no longer resolves through its mounted filesystem.
    #[error("volume unavailable")]
    Unmounted,
    /// The entry exists but is not a readable regular file.
    #[error("unreadable")]
    Unreadable,
    /// Another filesystem I/O failure prevented verification.
    #[error("filesystem I/O failure")]
    Io,
    /// The bounded check deadline elapsed.
    #[error("timed out")]
    TimedOut,
    /// Runtime shutdown cancelled verification.
    #[error("cancelled")]
    Cancelled,
}

impl AttachmentAccessFailure {
    /// Stable content-free diagnostic spelling.
    #[must_use]
    pub const fn diagnostic_code(self) -> &'static str {
        match self {
            Self::Missing => "missing",
            Self::PermissionDenied => "permission_denied",
            Self::Unmounted => "unmounted",
            Self::Unreadable => "unreadable",
            Self::Io => "io",
            Self::TimedOut => "timed_out",
            Self::Cancelled => "cancelled",
        }
    }

    /// Parse one stable internal worker-protocol spelling.
    #[must_use]
    pub(crate) fn from_diagnostic_code(code: &str) -> Option<Self> {
        match code {
            "missing" => Some(Self::Missing),
            "permission_denied" => Some(Self::PermissionDenied),
            "unmounted" => Some(Self::Unmounted),
            "unreadable" => Some(Self::Unreadable),
            "io" => Some(Self::Io),
            "timed_out" => Some(Self::TimedOut),
            "cancelled" => Some(Self::Cancelled),
            _ => None,
        }
    }
}

/// Scheduling purpose retained across the bounded worker lane.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AttachmentCheckPurpose {
    /// Quiet presentation refresh.
    Background,
    /// Mandatory fresh check before one submission journal exists.
    SubmissionPreflight(SubmissionId),
}

/// One bounded, ordered filesystem request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AttachmentCheckBatch {
    /// Process-local monotonically increasing request identity.
    pub id: u64,
    /// Background or submission-critical purpose.
    pub purpose: AttachmentCheckPurpose,
    /// Ordered exact attachment revisions to verify.
    pub checks: Vec<AttachmentCheckKey>,
    /// Overall batch deadline.
    pub timeout: Duration,
}

/// Result for one exact requested attachment revision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AttachmentCheckResult {
    /// Exact requested identity, used to reject stale results.
    pub key: AttachmentCheckKey,
    /// Accessible on success, otherwise one diagnostic-only failure reason.
    pub result: Result<(), AttachmentAccessFailure>,
}

/// Ordered completion for one batch.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AttachmentCheckBatchResult {
    /// Matching process-local request identity.
    pub id: u64,
    /// Matching scheduling purpose.
    pub purpose: AttachmentCheckPurpose,
    /// One result for every requested key, in request order.
    pub results: Vec<AttachmentCheckResult>,
}

/// Filesystem-independent capability used by the bounded accessibility lane.
pub trait AttachmentAccessibility: Send {
    /// Prove that one exact path currently names a readable regular file.
    ///
    /// # Errors
    ///
    /// Returns a typed diagnostic reason. Every failure is user-visible only as
    /// binary inaccessible health.
    fn check(&mut self, path: &Path) -> Result<(), AttachmentAccessFailure>;
}
