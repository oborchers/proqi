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

/// Sequences of forwarded captures awaiting their durable acknowledgement.
#[derive(Debug, Default)]
pub(in crate::ui::app) struct CaptureAnnouncements {
    pending: Vec<OperationSequence>,
    /// Sequence the latest owner-control mutation added, until the next one.
    latest: Option<OperationSequence>,
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
        self.latest = batch
            .and_then(|batch| batch.sequence())
            .filter(|_| announced);
        if let Some(sequence) = self.latest {
            self.pending.push(sequence);
        }
    }

    /// Forget the latest mutation's capture when effect validation rolled it back.
    ///
    /// Its sequence is then reused by the next mutation, whose acknowledgement
    /// must not be announced as a capture.
    pub(in crate::ui::app) fn roll_back_latest(&mut self) {
        if let Some(sequence) = self.latest.take() {
            self.pending.retain(|pending| *pending != sequence);
        }
    }

    /// Settle one acknowledgement and report whether a capture became durable.
    ///
    /// A failed save stays pending, with every other failed capture, so a
    /// successful retry still counts it. Successes are accepted strictly in
    /// sequence order, so a pending sequence below a successful one can never
    /// succeed later, for example after its request was rejected, and is pruned.
    pub(in crate::ui::app) fn acknowledge(
        &mut self,
        sequence: OperationSequence,
        succeeded: bool,
    ) -> bool {
        if !succeeded {
            return false;
        }
        let announced = self.pending.contains(&sequence);
        self.pending.retain(|pending| *pending > sequence);
        announced
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
