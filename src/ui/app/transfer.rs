//! Searchable cross-session destination picker and completion handling.

use crate::{
    application::{Action, Effect},
    domain::BoardOperationKind,
    ports::{
        editor::CursorMovement,
        environment::{Clock, IdGenerator},
        store::{SessionHit, StoreError},
        transfer::SessionTransferRequest,
    },
};

use super::{BoardApp, UiInput, UiKey, pending_types::EditFlush, query::QueryEditor};

#[path = "transfer/view.rs"]
mod view;
use view::SessionHitLabel as _;

pub(super) struct TransferState {
    generation: u64,
    query: QueryEditor,
    sessions: Vec<SessionHit>,
    selected: usize,
    scroll: usize,
    source_thought_id: crate::domain::ThoughtId,
    remove_source: bool,
    loading: bool,
}

impl BoardApp {
    pub(super) fn begin_session_transfer(
        &mut self,
        remove_source: bool,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        self.deactivate_range_latch();
        let Some(source_thought_id) = self.active_thought_id() else {
            self.set_warning("select a thought before sending it to another session");
            return Vec::new();
        };
        let mut effects = match self.flush_edit_boundary(ids, clock) {
            EditFlush::Complete(effects) => effects,
            EditFlush::Blocked(effects) => return effects,
        };
        self.transfer_generation = self.transfer_generation.wrapping_add(1);
        let generation = self.transfer_generation;
        self.transfer = Some(TransferState {
            generation,
            query: QueryEditor::default(),
            sessions: Vec::new(),
            selected: 0,
            scroll: 0,
            source_thought_id,
            remove_source,
            loading: true,
        });
        effects.push(Effect::DiscoverTransferSessions { generation });
        effects
    }

    pub(crate) fn complete_transfer_discovery(
        &mut self,
        generation: u64,
        result: Result<Vec<SessionHit>, StoreError>,
    ) {
        let Some(state) = &mut self.transfer else {
            return;
        };
        if state.generation != generation {
            return;
        }
        state.loading = false;
        match result {
            Ok(sessions) if sessions.is_empty() => {
                self.transfer = None;
                self.set_warning("no other resumable Proqi session is available");
            }
            Ok(sessions) => {
                state.sessions = sessions;
                state.selected = state.selected.min(state.matches().len().saturating_sub(1));
                state.scroll = state.scroll.min(state.selected);
            }
            Err(error) => {
                self.transfer = None;
                self.set_error(format!("could not list destination sessions: {error}"));
            }
        }
    }

    pub(crate) fn complete_session_transfer(
        &mut self,
        request: &SessionTransferRequest,
        result: Result<crate::application::ThoughtMutation, String>,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        if request.remove_source {
            self.pending_transfer_removals.remove(&request.operation_id);
        }
        match result {
            Err(error) => {
                self.set_error(format!("thought was not sent: {error}"));
                Vec::new()
            }
            Ok(_) if !request.remove_source => {
                self.set_success("thought sent to the destination session");
                Vec::new()
            }
            Ok(_)
                if self
                    .state
                    .board
                    .thought(request.source_thought_id)
                    .is_some_and(|thought| {
                        thought.content != request.content
                            || thought.annotations != request.annotations
                    }) =>
            {
                self.set_info("thought sent; source changed and was kept");
                Vec::new()
            }
            Ok(_)
                if self
                    .state
                    .board
                    .thought(request.source_thought_id)
                    .is_none() =>
            {
                self.set_info("thought sent; source was already removed");
                Vec::new()
            }
            Ok(_) => {
                self.set_info("thought sent; removing the source");
                self.reduce_with_empty_transition(
                    Action::DeleteThought {
                        operation_id: ids.operation_id(),
                        thought_id: request.source_thought_id,
                        kind: BoardOperationKind::TransferAndRemove,
                        at: clock.now(),
                    },
                    crate::application::EmptyBoardTransition::ComposeAfterLocalRemoval,
                )
            }
        }
    }

    pub(super) fn transfer_view(&self) -> Option<(String, Vec<String>, usize)> {
        let state = self.transfer.as_ref()?;
        let matches = state.matches();
        let entries = if state.loading {
            vec!["Loading sessions...".to_owned()]
        } else {
            matches
                .iter()
                .skip(state.scroll)
                .map(|hit| hit.label())
                .collect()
        };
        Some((
            state.query.text().to_owned(),
            entries,
            state.selected.saturating_sub(state.scroll),
        ))
    }

    pub(super) fn transfer_match_count(&self) -> usize {
        self.transfer
            .as_ref()
            .map_or(0, |state| state.matches().len())
    }

    pub(super) fn transfer_overflow(&self, visible: usize) -> (bool, bool) {
        self.transfer.as_ref().map_or((false, false), |state| {
            (
                state.scroll > 0,
                state.scroll.saturating_add(visible) < state.matches().len(),
            )
        })
    }

    pub(super) fn handle_transfer_input(
        &mut self,
        input: &UiInput,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        let UiInput::Key(key) = input else {
            return match input {
                UiInput::Pointer(pointer) => match pointer.kind {
                    crate::ui::PointerKind::ScrollUp => {
                        self.move_transfer(-1);
                        Vec::new()
                    }
                    crate::ui::PointerKind::ScrollDown => {
                        self.move_transfer(1);
                        Vec::new()
                    }
                    _ => self.handle_pointer(*pointer, ids, clock),
                },
                UiInput::Paste(value) => self.update_transfer_query(|query| query.paste(value)),
                UiInput::PasteAnnotated(payload) => {
                    self.update_transfer_query(|query| query.paste(&payload.content))
                }
                _ => Vec::new(),
            };
        };
        match *key {
            UiKey::Escape => self.transfer = None,
            UiKey::Enter => return self.choose_transfer(ids),
            UiKey::Backspace => {
                self.update_transfer_query(QueryEditor::backspace);
            }
            UiKey::FastNavigation { direction, .. } => self.move_transfer(direction.delta()),
            UiKey::Move {
                movement: CursorMovement::VisualUp,
                ..
            } => self.move_transfer(-1),
            UiKey::Move {
                movement: CursorMovement::VisualDown,
                ..
            } => self.move_transfer(1),
            UiKey::Move {
                movement,
                extend_selection,
            } => {
                self.update_transfer_query(|query| {
                    query.move_cursor_with_selection(movement, extend_selection);
                });
            }
            UiKey::Delete | UiKey::ModifiedDelete => {
                self.update_transfer_query(QueryEditor::delete);
            }
            UiKey::Character(character) if !character.is_control() => {
                self.update_transfer_query(|query| query.insert_char(character));
            }
            UiKey::UnmodifiedSpace => {
                self.update_transfer_query(|query| query.insert_char(' '));
            }
            UiKey::SelectAll => {
                self.update_transfer_query(QueryEditor::select_all);
            }
            UiKey::Undo => {
                self.update_transfer_query(|query| {
                    query.undo();
                });
            }
            UiKey::Redo => {
                self.update_transfer_query(|query| {
                    query.redo();
                });
            }
            _ => {}
        }
        Vec::new()
    }

    pub(super) fn choose_transfer_visible(
        &mut self,
        index: usize,
        ids: &mut impl IdGenerator,
    ) -> Vec<Effect> {
        if let Some(state) = &mut self.transfer {
            state.selected = state.scroll.saturating_add(index);
        }
        self.choose_transfer(ids)
    }

    fn choose_transfer(&mut self, ids: &mut impl IdGenerator) -> Vec<Effect> {
        if self.transfer.as_ref().is_some_and(|state| state.loading) {
            return Vec::new();
        }
        let request = self.transfer.as_ref().and_then(|state| {
            let destination = state.matches().get(state.selected)?.id;
            let thought = self.state.board.thought(state.source_thought_id)?;
            Some(SessionTransferRequest {
                destination_session_id: destination,
                source_thought_id: thought.id,
                operation_id: ids.operation_id(),
                content: thought.content.clone(),
                annotations: thought.annotations.clone(),
                remove_source: state.remove_source,
            })
        });
        self.transfer = None;
        request.map_or_else(Vec::new, |request| {
            if request.remove_source {
                self.pending_transfer_removals.insert(request.operation_id);
            }
            vec![Effect::TransferThought(request)]
        })
    }

    fn update_transfer_query(&mut self, update: impl FnOnce(&mut QueryEditor)) -> Vec<Effect> {
        if let Some(state) = &mut self.transfer {
            update(&mut state.query);
            state.selected = 0;
            state.scroll = 0;
        }
        Vec::new()
    }

    fn move_transfer(&mut self, delta: isize) {
        let visible = self
            .layout
            .as_ref()
            .and_then(|layout| layout.overlay.as_ref())
            .map_or(1, |overlay| overlay.items.len().max(1));
        let Some(state) = &mut self.transfer else {
            return;
        };
        state.selected = state
            .selected
            .saturating_add_signed(delta)
            .min(state.matches().len().saturating_sub(1));
        state.scroll = crate::ui::paging::first_visible(state.selected, state.scroll, visible);
        self.layout = None;
    }

    pub(super) fn ensure_transfer_visible(&mut self, visible: usize) {
        let Some(state) = &mut self.transfer else {
            return;
        };
        state.selected = state.selected.min(state.matches().len().saturating_sub(1));
        state.scroll = crate::ui::paging::first_visible(state.selected, state.scroll, visible);
    }
}

impl TransferState {
    pub(super) const fn query_cursor(&self) -> usize {
        self.query.cursor()
    }

    pub(super) const fn query_selection(&self) -> Option<super::query::QuerySelection> {
        self.query.selection()
    }

    fn matches(&self) -> Vec<&SessionHit> {
        let query = self.query.text().to_lowercase();
        self.sessions
            .iter()
            .filter(|hit| {
                query.is_empty()
                    || hit
                        .name
                        .as_deref()
                        .unwrap_or_default()
                        .to_lowercase()
                        .contains(&query)
                    || hit
                        .last_opened_cwd
                        .to_string_lossy()
                        .to_lowercase()
                        .contains(&query)
                    || hit.excerpt.to_lowercase().contains(&query)
            })
            .collect()
    }
}

#[cfg(test)]
#[path = "transfer/tests/paging.rs"]
mod paging_tests;

#[cfg(test)]
#[path = "transfer/tests.rs"]
mod tests;
