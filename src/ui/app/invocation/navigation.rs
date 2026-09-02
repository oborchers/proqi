use super::{BoardApp, view};

impl BoardApp {
    pub(super) fn move_invocation(&mut self, delta: isize) {
        let Some(popup) = self.invocation_popup.as_ref() else {
            return;
        };
        let choices = self.invocation_choices(popup);
        let count = choices.len();
        let row_budget = self
            .layout
            .as_ref()
            .and_then(|layout| layout.overlay.as_ref())
            .map_or(1, |overlay| {
                usize::from(overlay.area.height.saturating_sub(3)).max(1)
            });
        let Some(popup) = &mut self.invocation_popup else {
            return;
        };
        popup.selected = popup
            .selected
            .saturating_add_signed(delta)
            .min(count.saturating_sub(1));
        if popup.selected < popup.scroll {
            popup.scroll = popup.selected;
        } else {
            popup.scroll =
                view::scroll_for_selection(&choices, popup.selected, popup.scroll, row_budget);
        }
        self.layout = None;
    }

    pub(in crate::ui::app) fn ensure_invocation_visible(&mut self, row_budget: usize) {
        let Some(popup) = self.invocation_popup.as_ref() else {
            return;
        };
        let choices = self.invocation_choices(popup);
        let count = choices.len();
        let Some(popup) = &mut self.invocation_popup else {
            return;
        };
        popup.selected = popup.selected.min(count.saturating_sub(1));
        popup.scroll =
            view::scroll_for_selection(&choices, popup.selected, popup.scroll, row_budget);
    }

    pub(super) fn clamp_invocation_popup(&mut self) {
        self.ensure_invocation_visible(usize::MAX);
    }
}
