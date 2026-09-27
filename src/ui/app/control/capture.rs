//! Quiet presentation of explicit captures forwarded through owner control.
//!
//! A forwarded capture appends at the Board end like a Screenshot Inbox
//! capture and shares its `N new captures` status. The status appears only
//! after the capture's operation is durable, never for a rejected, replayed,
//! or failed request.

use crate::{
    domain::OperationSequence,
    ports::{
        control::{AddAnnouncement, ControlMutation},
        store::OperationBatch,
    },
};

use super::BoardApp;

/// Bound on unacknowledged forwarded captures; the persistence lane is ordered.
const MAX_PENDING: usize = 64;

/// Sequences of forwarded captures awaiting their durable acknowledgement.
#[derive(Debug, Default)]
pub(in crate::ui::app) struct CaptureAnnouncements {
    pending: Vec<OperationSequence>,
}

impl CaptureAnnouncements {
    /// Remember a capture whose owner-control mutation produced this batch.
    pub(in crate::ui::app) fn track(
        &mut self,
        mutation: &ControlMutation,
        batch: Option<OperationBatch>,
    ) {
        let announced = matches!(
            mutation,
            ControlMutation::Add {
                announcement: Some(AddAnnouncement::Capture),
                ..
            }
        );
        if let (true, Some(sequence)) = (announced, batch.and_then(|batch| batch.sequence())) {
            if self.pending.len() == MAX_PENDING {
                self.pending.remove(0);
            }
            self.pending.push(sequence);
        }
    }

    /// Settle one acknowledgement and report whether a capture became durable.
    pub(in crate::ui::app) fn acknowledge(
        &mut self,
        sequence: OperationSequence,
        succeeded: bool,
    ) -> bool {
        let Some(index) = self.pending.iter().position(|pending| *pending == sequence) else {
            return false;
        };
        self.pending.remove(index);
        succeeded
    }
}

impl BoardApp {
    /// A capture appends at the Board end; any other add keeps its requested position.
    pub(super) fn announced_position(
        &self,
        announcement: Option<AddAnnouncement>,
        position: Option<usize>,
    ) -> Option<usize> {
        match announcement {
            Some(AddAnnouncement::Capture) => {
                position.or_else(|| Some(self.state.board.live_items().len()))
            }
            None => position,
        }
    }
}
