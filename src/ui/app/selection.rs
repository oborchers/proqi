//! Explicit discontiguous and anchored contiguous board selection state.

use std::collections::BTreeSet;

use crate::{application::Action, domain::ThoughtId};

use super::BoardApp;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct BoardRange {
    anchor: ThoughtId,
    endpoint: ThoughtId,
}

#[derive(Default)]
pub(super) struct BoardSelection {
    selected: BTreeSet<ThoughtId>,
    range: Option<BoardRange>,
    latched: bool,
}

impl BoardSelection {
    pub(super) fn contains(&self, thought_id: ThoughtId) -> bool {
        self.selected.contains(&thought_id)
    }

    pub(super) fn is_empty(&self) -> bool {
        self.selected.is_empty()
    }

    pub(super) fn len(&self) -> usize {
        self.selected.len()
    }

    pub(super) fn selected_in(&self, order: &[ThoughtId]) -> Vec<ThoughtId> {
        order
            .iter()
            .copied()
            .filter(|thought_id| self.selected.contains(thought_id))
            .collect()
    }

    pub(super) fn clear(&mut self) {
        self.selected.clear();
        self.range = None;
        self.latched = false;
    }

    pub(super) fn replace_arbitrary(&mut self, selected: impl IntoIterator<Item = ThoughtId>) {
        self.selected = selected.into_iter().collect();
        self.range = None;
        self.latched = false;
    }

    fn toggle_arbitrary(&mut self, thought_id: ThoughtId) {
        self.range = None;
        self.latched = false;
        if !self.selected.remove(&thought_id) {
            self.selected.insert(thought_id);
        }
    }

    fn range_anchor(&self, fallback: ThoughtId) -> ThoughtId {
        self.range.map_or(fallback, |range| range.anchor)
    }

    fn set_range(&mut self, order: &[ThoughtId], anchor: ThoughtId, endpoint: ThoughtId) {
        let Some(anchor_index) = order.iter().position(|id| *id == anchor) else {
            self.clear();
            return;
        };
        let Some(endpoint_index) = order.iter().position(|id| *id == endpoint) else {
            self.clear();
            return;
        };
        let (start, end) = if anchor_index <= endpoint_index {
            (anchor_index, endpoint_index)
        } else {
            (endpoint_index, anchor_index)
        };
        self.selected = order[start..=end].iter().copied().collect();
        self.range = Some(BoardRange { anchor, endpoint });
    }

    pub(super) fn reconcile(&mut self, order: &[ThoughtId]) {
        if let Some(range) = self.range {
            self.set_range(order, range.anchor, range.endpoint);
        } else {
            self.selected
                .retain(|thought_id| order.contains(thought_id));
        }
    }

    fn activate_latch(&mut self, order: &[ThoughtId], focused: ThoughtId) {
        let anchor = self.range_anchor(focused);
        let endpoint = self.range.map_or(focused, |range| range.endpoint);
        self.set_range(order, anchor, endpoint);
        self.latched = self.range.is_some();
    }

    fn deactivate_latch(&mut self) {
        self.latched = false;
    }

    fn is_range(&self) -> bool {
        self.range.is_some()
    }
}

impl BoardApp {
    pub(super) fn selection_is_empty(&self) -> bool {
        self.selection.is_empty()
    }

    pub(super) fn selection_len(&self) -> usize {
        self.selection.len()
    }

    pub(super) fn clear_board_selection(&mut self) {
        self.selection.clear();
        self.hovered = None;
        self.layout = None;
    }

    pub(super) fn replace_board_selection(
        &mut self,
        selected: impl IntoIterator<Item = ThoughtId>,
    ) {
        self.selection.replace_arbitrary(selected);
        self.hovered = None;
        self.layout = None;
    }

    pub(super) fn toggle_selection(&mut self) {
        let Some(thought_id) = self.state.focused_thought else {
            return;
        };
        self.selection.toggle_arbitrary(thought_id);
        self.hovered = None;
        self.layout = None;
    }

    pub(super) fn select_all_thoughts(&mut self) {
        let order = self.live_thought_ids();
        self.selection.replace_arbitrary(order);
        self.hovered = None;
        self.layout = None;
    }

    pub(super) fn activate_range_latch(&mut self) {
        let Some(focused) = self.state.focused_thought else {
            return;
        };
        let order = self.live_thought_ids();
        self.selection.activate_latch(&order, focused);
        self.hovered = None;
        self.layout = None;
    }

    pub(super) fn deactivate_range_latch(&mut self) {
        self.selection.deactivate_latch();
    }

    pub(super) fn range_latched(&self) -> bool {
        self.selection.latched
    }

    pub(super) fn clear_range_for_focus_change(&mut self) {
        if self.selection.is_range() {
            self.clear_board_selection();
        }
    }

    pub(super) fn extend_range_by(&mut self, delta: isize) {
        let order = self.live_thought_ids();
        let Some(focused) = self.state.focused_thought else {
            return;
        };
        let Some(current) = order.iter().position(|id| *id == focused) else {
            return;
        };
        let target = current
            .saturating_add_signed(delta)
            .min(order.len().saturating_sub(1));
        if let Some(endpoint) = order.get(target).copied() {
            self.extend_range_to(endpoint);
        }
    }

    pub(super) fn extend_range_to(&mut self, endpoint: ThoughtId) {
        let order = self.live_thought_ids();
        let Some(focused) = self.state.focused_thought else {
            return;
        };
        let anchor = self.selection.range_anchor(focused);
        self.selection.set_range(&order, anchor, endpoint);
        if self.selection.is_range() {
            self.insertion_focus = super::InsertionFocus::Inactive;
            self.manual_board_scroll = false;
            self.first_visible_row = 0;
            let _effects = self.reduce(Action::FocusThought(Some(endpoint)));
            self.hovered = None;
            self.layout = None;
        }
    }

    pub(super) fn move_focus_outside_range(&mut self, delta: isize) {
        self.clear_range_for_focus_change();
        self.move_focus(delta);
    }

    fn live_thought_ids(&self) -> Vec<ThoughtId> {
        self.state
            .board
            .live_thoughts()
            .into_iter()
            .map(|thought| thought.id)
            .collect()
    }
}
