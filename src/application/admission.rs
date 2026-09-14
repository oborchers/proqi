//! Typed accounting for asynchronous intentions that can still allocate a session sequence.

/// One pending asynchronous intention that may later mutate the live board.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PendingMutationIntent {
    /// A confirmed clipboard write may delete a board thought or editor selection.
    ClipboardCut,
    /// A pending clipboard read may create a thought or edit the active thought.
    ClipboardPaste,
    /// A submission completion may persist integration context or remove source thoughts.
    SubmissionCompletion,
    /// A successful cross-session transfer may remove its source thought.
    TransferRemove,
}

/// Bounded typed counts of pending asynchronous sequence producers.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PendingMutationIntents {
    clipboard_cuts: usize,
    clipboard_pastes: usize,
    submission_completions: usize,
    transfer_removals: usize,
}

impl PendingMutationIntents {
    /// Record pending intentions of one known kind.
    pub fn add(&mut self, intent: PendingMutationIntent, count: usize) {
        let target = match intent {
            PendingMutationIntent::ClipboardCut => &mut self.clipboard_cuts,
            PendingMutationIntent::ClipboardPaste => &mut self.clipboard_pastes,
            PendingMutationIntent::SubmissionCompletion => &mut self.submission_completions,
            PendingMutationIntent::TransferRemove => &mut self.transfer_removals,
        };
        *target = target.saturating_add(count);
    }

    /// Total pending sequence-producing completions.
    #[must_use]
    pub const fn total(self) -> usize {
        self.clipboard_cuts
            .saturating_add(self.clipboard_pastes)
            .saturating_add(self.submission_completions)
            .saturating_add(self.transfer_removals)
    }

    /// Whether no asynchronous completion can still allocate a sequence.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.total() == 0
    }
}
