//! Canonical view of asynchronous UI intentions that may still allocate a sequence.

use crate::application::{PendingMutationIntent, PendingMutationIntents};

use super::BoardApp;

impl BoardApp {
    /// Typed pending intentions consulted by every runner mutation-admission path.
    pub(crate) fn pending_mutation_intents(&self) -> PendingMutationIntents {
        let mut pending = PendingMutationIntents::default();
        pending.add(
            PendingMutationIntent::ClipboardCut,
            self.state.pending_board_cut_count()
                + self
                    .pending_editor_clipboard
                    .values()
                    .filter(|item| item.intent == crate::application::ClipboardIntent::Cut)
                    .count(),
        );
        pending.add(
            PendingMutationIntent::ClipboardPaste,
            self.pending_clipboard_reads.len(),
        );
        pending.add(
            PendingMutationIntent::SubmissionCompletion,
            self.pending_submissions
                .len()
                .saturating_add(self.deferred_submissions.len())
                .saturating_add(self.preflight_submissions.len()),
        );
        pending.add(
            PendingMutationIntent::TransferRemove,
            self.pending_transfer_removals.len(),
        );
        pending
    }
}
