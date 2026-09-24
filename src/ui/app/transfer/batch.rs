//! Exact selected-thought cohort transfer and source completion.

use crate::{
    application::{Action, Effect},
    domain::BoardOperationKind,
    ports::{
        environment::{Clock, IdGenerator},
        transfer::{SessionTransferBatchRequest, TransferItem},
    },
};

use super::BoardApp;

impl BoardApp {
    pub(super) fn choose_transfer_batch(&mut self, ids: &mut impl IdGenerator) -> Vec<Effect> {
        let selected = self
            .transfer
            .as_ref()
            .map(|state| state.source_thought_ids.clone());
        let destination = self
            .transfer
            .as_ref()
            .and_then(|state| state.matches().get(state.selected).map(|hit| hit.id));
        let remove_source = self
            .transfer
            .as_ref()
            .is_some_and(|state| state.remove_source);
        if let Some(existing) = self
            .pending_transfer_batches
            .values()
            .find(|request| {
                Some(request.destination_session_id) == destination
                    && request.remove_source == remove_source
                    && selected.as_ref().is_some_and(|selected| {
                        request
                            .items
                            .iter()
                            .map(|item| item.source_thought_id)
                            .eq(selected.iter().copied())
                    })
            })
            .cloned()
        {
            self.transfer = None;
            return vec![Effect::TransferThoughts(existing)];
        }
        if self.pending_transfer_batches.values().any(|request| {
            selected.as_ref().is_some_and(|selected| {
                request
                    .items
                    .iter()
                    .any(|item| selected.contains(&item.source_thought_id))
            })
        }) {
            self.transfer = None;
            self.set_warning("thoughts already have a transfer in progress");
            return Vec::new();
        }
        let request = self.transfer.as_ref().and_then(|state| {
            let destination_session_id = state.matches().get(state.selected)?.id;
            let items = state
                .source_thought_ids
                .iter()
                .map(|source_thought_id| {
                    let thought = self.state.board.thought(*source_thought_id)?;
                    Some(TransferItem {
                        source_thought_id: *source_thought_id,
                        destination_thought_id: ids.thought_id(),
                        content: thought.content.clone(),
                        annotations: thought.annotations.clone(),
                        name: thought.name.clone(),
                    })
                })
                .collect::<Option<Vec<_>>>()?;
            Some(SessionTransferBatchRequest {
                source_session_id: self.state.board.session.id,
                destination_session_id,
                operation_id: ids.operation_id(),
                removal_operation_id: ids.operation_id(),
                items,
                remove_source: state.remove_source,
            })
        });
        self.transfer = None;
        request.map_or_else(Vec::new, |request| {
            self.pending_transfer_batches
                .insert(request.operation_id, request.clone());
            vec![Effect::TransferThoughts(request)]
        })
    }

    pub(crate) fn recover_pending_transfers(
        &mut self,
        requests: Vec<SessionTransferBatchRequest>,
    ) -> Vec<Effect> {
        requests
            .into_iter()
            .map(|request| {
                self.pending_transfer_batches
                    .insert(request.operation_id, request.clone());
                Effect::TransferThoughts(request)
            })
            .collect()
    }

    pub(crate) fn complete_transfer_journal(&mut self, operation_id: crate::domain::OperationId) {
        self.pending_transfer_batches.remove(&operation_id);
    }

    pub(crate) fn complete_session_transfer_batch(
        &mut self,
        request: &SessionTransferBatchRequest,
        result: Result<crate::ports::store::CommitReceipt, String>,
        _ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        if let Err(error) = result {
            if request.items.len() == 1 {
                self.set_error(format!("thought was not sent: {error}"));
            } else {
                self.set_error(format!("selected thoughts were not sent: {error}"));
            }
            return Vec::new();
        }
        if !request.remove_source {
            if request.items.len() == 1 {
                self.set_success("thought sent to the destination session");
            } else {
                self.set_success("selected thoughts sent to the destination session");
            }
            return vec![Effect::FinishTransfer {
                request: request.clone(),
                removal: None,
                reason: "kept",
            }];
        }
        let unchanged = request.items.iter().all(|item| {
            self.state
                .board
                .thought(item.source_thought_id)
                .is_some_and(|thought| {
                    thought.is_live()
                        && thought.content == item.content
                        && thought.annotations == item.annotations
                        && thought.name == item.name
                })
        });
        if !unchanged {
            if request.items.len() == 1 {
                let source_id = request.items[0].source_thought_id;
                let removed = self
                    .state
                    .board
                    .thought(source_id)
                    .is_none_or(|thought| !thought.is_live());
                self.set_info(stale_single_source_message(removed));
            } else {
                self.set_warning("selected thoughts were sent; changed sources were kept");
            }
            return vec![Effect::FinishTransfer {
                request: request.clone(),
                removal: None,
                reason: "source_changed",
            }];
        }
        if request.items.len() == 1 {
            self.set_info("thought sent; removing the source");
        }
        self.reduce_with_empty_transition(
            Action::DeleteThoughts {
                operation_id: request.removal_operation_id,
                thought_ids: request
                    .items
                    .iter()
                    .map(|item| item.source_thought_id)
                    .collect(),
                kind: BoardOperationKind::TransferAndRemove,
                at: clock.now(),
            },
            crate::application::EmptyBoardTransition::ComposeAfterLocalRemoval,
        )
        .into_iter()
        .map(|effect| match effect {
            Effect::CommitBoardOperation(operation) => Effect::FinishTransfer {
                request: request.clone(),
                removal: Some(operation),
                reason: "removed",
            },
            other => other,
        })
        .collect()
    }
}

const fn stale_single_source_message(removed: bool) -> &'static str {
    if removed {
        "thought sent; source was already removed"
    } else {
        "thought sent; source changed and was kept"
    }
}
