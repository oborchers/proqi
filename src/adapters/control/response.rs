//! Owner responses with optional transport-delivery confirmation.

use std::sync::mpsc::{Receiver, SyncSender, TryRecvError, sync_channel};

use crate::ports::control::{ControlRequest, ControlResponse, ControlResult};

/// Result of the bounded response-frame write for a confirmed control request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ControlDelivery {
    Delivered,
    Failed,
}

/// One response-delivery notification owned by the reducer lane.
pub(crate) struct ControlDeliveryReceipt(Receiver<ControlDelivery>);

impl ControlDeliveryReceipt {
    pub(crate) fn try_recv(&self) -> Result<ControlDelivery, TryRecvError> {
        self.0.try_recv()
    }
}

/// One verified request waiting for the owner reducer.
pub(crate) struct ControlEnvelope {
    pub(crate) request: ControlRequest,
    response: SyncSender<PendingResponse>,
}

impl ControlEnvelope {
    /// Respond exactly once to the waiting transport request.
    pub(crate) fn respond(self, result: ControlResult) {
        let _sent = self
            .response
            .send(PendingResponse::new(response(&self.request, result), None));
    }

    /// Respond and return confirmation that the frame reached the local socket.
    pub(crate) fn respond_confirmed(self, result: ControlResult) -> ControlDeliveryReceipt {
        let (sender, receiver) = sync_channel(1);
        let _sent = self.response.send(PendingResponse::new(
            response(&self.request, result),
            Some(sender),
        ));
        ControlDeliveryReceipt(receiver)
    }
}

pub(crate) struct PendingResponse {
    pub(crate) response: ControlResponse,
    delivery: Option<SyncSender<ControlDelivery>>,
}

impl PendingResponse {
    fn new(response: ControlResponse, delivery: Option<SyncSender<ControlDelivery>>) -> Self {
        Self { response, delivery }
    }

    pub(crate) const fn is_confirmed(&self) -> bool {
        self.delivery.is_some()
    }

    pub(crate) fn complete(self, delivered: bool) {
        if let Some(sender) = self.delivery {
            let outcome = if delivered {
                ControlDelivery::Delivered
            } else {
                ControlDelivery::Failed
            };
            let _sent = sender.send(outcome);
        }
    }
}

pub(crate) fn pending(request: ControlRequest) -> (ControlEnvelope, Receiver<PendingResponse>) {
    let (sender, receiver) = sync_channel(1);
    (
        ControlEnvelope {
            request,
            response: sender,
        },
        receiver,
    )
}

fn response(request: &ControlRequest, result: ControlResult) -> ControlResponse {
    ControlResponse {
        protocol: request.protocol,
        request_id: request.request_id,
        result,
    }
}
