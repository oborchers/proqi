//! Closed semantic kinds for durable Board history.

use serde::{Deserialize, Serialize};

/// Kind of structural operation shown in history and diagnostics.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BoardOperationKind {
    /// Created a thought, including paste-to-create.
    Create,
    /// Deleted a thought without touching the clipboard.
    Delete,
    /// Deleted a thought after a successful clipboard write.
    Cut,
    /// Reordered one thought.
    Reorder,
    /// Changed the explicit collapse preference.
    Collapse,
    /// Duplicated one or more thoughts as one operation.
    Duplicate,
    /// Deleted after an accepted adjacent-agent submission.
    SubmitAndRemove,
    /// Deleted after the destination session durably accepted a transfer copy.
    TransferAndRemove,
    /// Split one thought at an exact logical cursor.
    Split,
    /// Extract one exact editor selection into a neighboring thought.
    Extract,
    /// Reflow one existing thought in place.
    Reflow,
    /// Merge a contiguous board selection into its first thought.
    Merge,
}

impl BoardOperationKind {
    /// Whether this operation participates in one thought's content timeline.
    ///
    /// Board-only presentation and ordering changes remain owned by Board even
    /// though their payloads mention thought identifiers.
    #[must_use]
    pub const fn belongs_to_thought_content(self) -> bool {
        matches!(
            self,
            Self::Create
                | Self::Delete
                | Self::Cut
                | Self::Duplicate
                | Self::SubmitAndRemove
                | Self::TransferAndRemove
                | Self::Split
                | Self::Extract
                | Self::Reflow
                | Self::Merge
        )
    }
}
