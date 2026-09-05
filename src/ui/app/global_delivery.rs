//! Searchable current-server target and explicit disposition chooser.

use crate::{
    application::Effect,
    domain::ThoughtId,
    ports::{
        agent::{
            AgentAvailability, AgentError, AgentFailureCode, AgentTarget, SubmissionDisposition,
        },
        editor::CursorMovement,
        environment::{Clock, IdGenerator},
    },
};

use super::{BoardApp, UiInput, UiKey, pending_types::EditFlush, query::QueryEditor};

mod view;

pub(super) struct GlobalDeliveryState {
    generation: u64,
    query: QueryEditor,
    selected: usize,
    scroll: usize,
    thought_ids: Vec<ThoughtId>,
    source_digests: Vec<[u8; 32]>,
    stage: GlobalDeliveryStage,
}

enum GlobalDeliveryStage {
    Targets {
        loading: bool,
        targets: Vec<AgentTarget>,
        failure: Option<AgentFailureCode>,
    },
    Disposition {
        target: Box<AgentTarget>,
    },
}

pub(in crate::ui) struct GlobalDeliveryChoiceView {
    pub(in crate::ui) primary: String,
    pub(in crate::ui) secondary: String,
    pub(in crate::ui) secondary_fallbacks: Vec<String>,
    pub(in crate::ui) protected_secondaries: Vec<String>,
    pub(in crate::ui) enabled: bool,
}

pub(in crate::ui) struct GlobalDeliveryView {
    pub(in crate::ui) title: &'static str,
    pub(in crate::ui) query: String,
    pub(in crate::ui) choices: Vec<GlobalDeliveryChoiceView>,
    pub(in crate::ui) selected: usize,
}

impl BoardApp {
    pub(super) fn begin_global_delivery(
        &mut self,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        let thought_ids = self.action_thought_ids();
        if thought_ids.is_empty() {
            self.set_warning("select a thought before submitting to an agent");
            return Vec::new();
        }
        let mut effects = match self.flush_edit_boundary(ids, clock) {
            EditFlush::Complete(effects) => effects,
            EditFlush::Blocked(effects) => return effects,
        };
        let source_digests = thought_ids
            .iter()
            .filter_map(|thought_id| self.current_thought_digest(*thought_id))
            .collect::<Vec<_>>();
        if source_digests.len() != thought_ids.len() {
            self.set_warning("board changed before agent discovery; thoughts kept");
            return effects;
        }
        self.global_delivery_generation = self.global_delivery_generation.wrapping_add(1);
        let generation = self.global_delivery_generation;
        self.global_delivery = Some(GlobalDeliveryState {
            generation,
            query: QueryEditor::default(),
            selected: 0,
            scroll: 0,
            thought_ids,
            source_digests,
            stage: GlobalDeliveryStage::Targets {
                loading: true,
                targets: Vec::new(),
                failure: None,
            },
        });
        effects.push(Effect::DiscoverGlobalAgents { generation });
        effects
    }

    /// Apply one generation-matched current-server discovery completion.
    pub fn complete_global_agent_discovery(
        &mut self,
        generation: u64,
        result: Result<Vec<AgentTarget>, AgentError>,
    ) {
        let Some(state) = &mut self.global_delivery else {
            return;
        };
        if state.generation != generation {
            return;
        }
        let GlobalDeliveryStage::Targets {
            loading,
            targets,
            failure,
        } = &mut state.stage
        else {
            return;
        };
        *loading = false;
        match result {
            Ok(mut discovered) => {
                discovered.sort_by(|left, right| {
                    left.workspace_label
                        .cmp(&right.workspace_label)
                        .then_with(|| left.tab_label.cmp(&right.tab_label))
                        .then_with(|| left.agent_name.cmp(&right.agent_name))
                        .then_with(|| left.workspace_id().cmp(right.workspace_id()))
                        .then_with(|| left.tab_id().cmp(right.tab_id()))
                        .then_with(|| left.pane_id().cmp(right.pane_id()))
                });
                *targets = discovered;
                *failure = None;
            }
            Err(error) => {
                targets.clear();
                *failure = Some(error.stable_code());
            }
        }
        state.clamp();
    }

    pub(super) fn handle_global_delivery_input(
        &mut self,
        input: &UiInput,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        let UiInput::Key(key) = input else {
            return match input {
                UiInput::Pointer(pointer) => match pointer.kind {
                    crate::ui::PointerKind::ScrollUp => {
                        self.move_global_delivery(-1);
                        Vec::new()
                    }
                    crate::ui::PointerKind::ScrollDown => {
                        self.move_global_delivery(1);
                        Vec::new()
                    }
                    _ => self.handle_pointer(*pointer, ids, clock),
                },
                UiInput::Paste(value) => {
                    self.update_global_query(|query| query.paste(value));
                    Vec::new()
                }
                UiInput::PasteAnnotated(payload) => {
                    self.update_global_query(|query| query.paste(&payload.content));
                    Vec::new()
                }
                _ => Vec::new(),
            };
        };
        match *key {
            UiKey::Escape => {
                self.global_delivery = None;
                self.set_info("agent submission cancelled");
            }
            UiKey::Enter => return self.choose_global_delivery(ids, clock),
            UiKey::Backspace => self.update_global_query(QueryEditor::backspace),
            UiKey::Delete | UiKey::ModifiedDelete => {
                self.update_global_query(QueryEditor::delete);
            }
            UiKey::FastNavigation { direction, .. } => {
                self.move_global_delivery(direction.delta());
            }
            UiKey::Move {
                movement: CursorMovement::VisualUp,
                ..
            } => self.move_global_delivery(-1),
            UiKey::Move {
                movement: CursorMovement::VisualDown,
                ..
            } => self.move_global_delivery(1),
            UiKey::Move { movement, .. } => {
                self.update_global_query(|query| query.move_cursor(movement));
            }
            UiKey::Character(character) if !character.is_control() => {
                self.update_global_query(|query| query.insert_char(character));
            }
            UiKey::UnmodifiedSpace => {
                self.update_global_query(|query| query.insert_char(' '));
            }
            _ => {}
        }
        Vec::new()
    }

    pub(super) fn choose_global_delivery_visible(
        &mut self,
        index: usize,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        if let Some(state) = &mut self.global_delivery {
            state.selected = state.scroll.saturating_add(index);
        }
        self.choose_global_delivery(ids, clock)
    }

    fn choose_global_delivery(
        &mut self,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        let no_choice_message = self
            .global_delivery
            .as_ref()
            .map_or("no compatible agent matches this search", |state| {
                state.no_choice_message()
            });
        let choice = self
            .global_delivery
            .as_ref()
            .and_then(GlobalDeliveryState::choice);
        match choice {
            Some(GlobalChoice::Target(target)) if target.can_submit() => {
                if let Some(state) = &mut self.global_delivery {
                    state.query = QueryEditor::default();
                    state.selected = 0;
                    state.scroll = 0;
                    state.stage = GlobalDeliveryStage::Disposition {
                        target: Box::new(target),
                    };
                }
                Vec::new()
            }
            Some(GlobalChoice::Target(target)) => {
                self.set_warning(unavailable_message(target.availability));
                Vec::new()
            }
            Some(GlobalChoice::Disposition(disposition, target, thought_ids, source_digests)) => {
                self.global_delivery = None;
                self.deliver_global_target(
                    &target,
                    disposition,
                    &thought_ids,
                    &source_digests,
                    ids,
                    clock,
                )
            }
            None => {
                self.set_warning(no_choice_message);
                Vec::new()
            }
        }
    }

    fn deliver_global_target(
        &mut self,
        target: &AgentTarget,
        disposition: SubmissionDisposition,
        thought_ids: &[ThoughtId],
        source_digests: &[[u8; 32]],
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        let sources_unchanged = thought_ids
            .iter()
            .zip(source_digests)
            .all(|(thought_id, digest)| self.current_thought_digest(*thought_id) == Some(*digest));
        if thought_ids.len() != source_digests.len() || !sources_unchanged {
            self.set_warning("source changed during agent selection; thoughts kept");
            return Vec::new();
        }
        if thought_ids.iter().any(|id| self.submission_locked(*id)) {
            self.set_warning("a selected thought already has a submission in progress");
            return Vec::new();
        }
        self.queue_submission(target, disposition, thought_ids, ids, clock)
    }

    fn update_global_query(&mut self, update: impl FnOnce(&mut QueryEditor)) {
        let Some(state) = &mut self.global_delivery else {
            return;
        };
        if !matches!(state.stage, GlobalDeliveryStage::Targets { .. }) {
            return;
        }
        update(&mut state.query);
        state.selected = 0;
        state.scroll = 0;
        state.clamp();
    }

    fn move_global_delivery(&mut self, delta: isize) {
        let visible = self
            .layout
            .as_ref()
            .and_then(|layout| layout.overlay.as_ref())
            .map_or(1, |overlay| overlay.items.len().max(1));
        let Some(state) = &mut self.global_delivery else {
            return;
        };
        state.selected = state
            .selected
            .saturating_add_signed(delta)
            .min(state.match_count().saturating_sub(1));
        state.scroll = crate::ui::paging::first_visible(state.selected, state.scroll, visible);
        self.layout = None;
    }

    pub(super) fn ensure_global_delivery_visible(&mut self, visible: usize) {
        let Some(state) = &mut self.global_delivery else {
            return;
        };
        state.clamp();
        state.scroll = crate::ui::paging::first_visible(state.selected, state.scroll, visible);
    }

    pub(in crate::ui) fn global_delivery_view(&self) -> Option<GlobalDeliveryView> {
        self.global_delivery.as_ref().map(GlobalDeliveryState::view)
    }

    pub(super) fn global_delivery_match_count(&self) -> usize {
        self.global_delivery
            .as_ref()
            .map_or(0, GlobalDeliveryState::match_count)
    }

    pub(super) fn global_delivery_query_cursor(&self) -> Option<usize> {
        self.global_delivery
            .as_ref()
            .map(|state| state.query.cursor())
    }

    pub(super) fn global_delivery_overflow(&self, visible: usize) -> (bool, bool) {
        self.global_delivery
            .as_ref()
            .map_or((false, false), |state| {
                (
                    state.scroll > 0,
                    state.scroll.saturating_add(visible) < state.match_count(),
                )
            })
    }
}

pub(super) enum GlobalChoice {
    Target(AgentTarget),
    Disposition(
        SubmissionDisposition,
        AgentTarget,
        Vec<ThoughtId>,
        Vec<[u8; 32]>,
    ),
}

const fn unavailable_message(availability: AgentAvailability) -> &'static str {
    match availability {
        AgentAvailability::Blocked => "agent is blocked; choose an available target",
        AgentAvailability::Unknown => "agent state is unknown; delivery is disabled",
        AgentAvailability::Launching => "agent is still launching; delivery is disabled",
        AgentAvailability::NotInteractive => "agent is not interactive yet; delivery is disabled",
        AgentAvailability::Available => "agent delivery is unavailable",
    }
}
