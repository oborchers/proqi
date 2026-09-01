//! Canonical view of asynchronous UI intentions that may still allocate a sequence.

use crate::{
    application::{PendingMutationIntent, PendingMutationIntents},
    ports::agent::SubmissionDisposition,
};

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
            PendingMutationIntent::SubmissionRemove,
            self.pending_submissions
                .values()
                .filter(|item| item.disposition == SubmissionDisposition::RemoveAfterSuccess)
                .count()
                + self
                    .deferred_submissions
                    .values()
                    .filter(|item| {
                        item.pending.disposition == SubmissionDisposition::RemoveAfterSuccess
                    })
                    .count()
                + self
                    .preflight_submissions
                    .values()
                    .filter(|item| {
                        item.pending.disposition == SubmissionDisposition::RemoveAfterSuccess
                    })
                    .count(),
        );
        pending.add(
            PendingMutationIntent::TransferRemove,
            self.pending_transfer_removals.len(),
        );
        pending
    }
}
