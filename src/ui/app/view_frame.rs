//! Authoritative frame geometry and responsive footer projection.

use ratatui_core::layout::Rect;

use crate::{
    application::{DurabilityState, InteractionMode},
    ports::editor::TextViewport,
    ui::LayoutSnapshot,
};

use super::{BoardApp, palette};

impl BoardApp {
    /// Prepare current frame geometry without changing the logical cursor.
    pub fn prepare_layout(&mut self, viewport: TextViewport) {
        self.viewport = viewport;
        if let Some((_, editor)) = &mut self.editor {
            editor.set_viewport(viewport);
        }
    }

    /// Recompute one authoritative frame layout and reflow the active editor.
    pub fn prepare_frame(&mut self, area: Rect) -> LayoutSnapshot {
        self.reset_overlay_activation_for_geometry(area);
        self.prepare_layout(TextViewport::new(
            area.width.saturating_sub(2).max(1),
            self.viewport.height,
        ));
        let mut presentation = self.build_frame_presentation();
        self.attach_editor_presentation(&mut presentation);
        let follow_insertion = self.insertion_focused() || self.compose_prompt_visible();
        let has_status = self.status_view().is_some()
            || matches!(self.state.durability, DurabilityState::Failed { .. });
        let (first, first_scroll) = crate::ui::layout::compute_for_app(
            &self.state,
            &presentation,
            area,
            follow_insertion,
            !self.agent_targets.is_empty(),
            has_status,
            self.settings.density,
            &self.settings.keybindings,
            self.board_viewport,
        );
        let height = self.focused_height(&first);
        self.prepare_layout(TextViewport::new(first.content_width, height));
        self.attach_editor_presentation(&mut presentation);
        let viewport = self.board_viewport.at(first_scroll.current);
        let (mut layout, scroll) = crate::ui::layout::compute_for_app(
            &self.state,
            &presentation,
            area,
            follow_insertion,
            !self.agent_targets.is_empty(),
            has_status,
            self.settings.density,
            &self.settings.keybindings,
            viewport,
        );
        self.configure_overlay(&mut layout);
        self.keep_overlay_selection_visible(&layout);
        self.configure_overlay(&mut layout);
        layout.configure_agent_controls_with_keys(
            &self.agent_targets,
            self.submission_mode(),
            self.interaction_mode(),
            &self.settings.keybindings,
        );
        let summary = self.footer_summary(layout.footer_context.width.saturating_sub(4));
        let session_id = self
            .settings
            .show_session_id
            .then(|| self.state.board.session.id.to_string());
        layout.configure_footer_summary(
            summary,
            self.session_display_name().to_owned(),
            session_id,
        );
        let final_height = self.focused_height(&layout);
        self.prepare_layout(TextViewport::new(layout.content_width, final_height));
        self.board_viewport = self.board_viewport.at(scroll.current);
        self.scroll_geometry = Some(scroll);
        self.frame_presentation = Some(presentation);
        self.layout = Some(layout.clone());
        self.clamp_help_scroll();
        self.clamp_release_highlights_scroll();
        layout
    }

    fn focused_height(&self, layout: &LayoutSnapshot) -> u16 {
        self.active_thought_id()
            .and_then(|id| layout.thought(id))
            .map_or(layout.board.height.max(1), |thought| {
                thought.text_area.height.max(1)
            })
    }

    fn configure_overlay(&self, layout: &mut LayoutSnapshot) {
        let screenshot_items = usize::from(self.screenshot.takeover.is_some()) * 2;
        let update_items = usize::from(self.update_prompt.is_some()) * 3;
        let highlight_rows = usize::from(self.release_highlights.is_some()).saturating_mul(
            self.release_highlights_row_count(layout.board.width.min(58).saturating_sub(2)),
        );
        let palette_items = self
            .palette
            .as_ref()
            .map_or(0, palette::PaletteState::match_count);
        let global_delivery_items = self.global_delivery_match_count();
        let invocation_items = self.invocation_match_count();
        let invocation_groups = self
            .invocation_view()
            .map_or_else(Vec::new, |(_, entries, _)| {
                entries
                    .iter()
                    .map(|entry| entry.group.is_some())
                    .collect::<Vec<_>>()
            });
        let search_items = self.search_match_count();
        let transfer_items = self.transfer_match_count();
        let preferred_rows = if self.screenshot.takeover.is_some() {
            2
        } else if self.update_prompt.is_some() {
            4
        } else if self.release_highlights.is_some() {
            highlight_rows.max(1)
        } else if self.help {
            let content_width = layout.board.width.min(58).saturating_sub(2);
            crate::ui::shortcuts::row_count(self, content_width)
        } else if self.rename.is_some() {
            2
        } else if self.invocation_popup.is_some() {
            invocation_groups
                .iter()
                .map(|grouped| 1 + usize::from(*grouped))
                .sum::<usize>()
                .max(2)
        } else if self.palette.is_some() {
            palette_items.max(2)
        } else if self.global_delivery.is_some() {
            global_delivery_items.max(2)
        } else if self.transfer.is_some() {
            transfer_items.max(2)
        } else if self.search.is_some() {
            search_items.max(2)
        } else {
            0
        };
        if self.invocation_popup.is_some() {
            layout.configure_grouped_overlay(&invocation_groups, preferred_rows);
        } else {
            layout.configure_overlay(
                screenshot_items
                    .max(palette_items)
                    .max(global_delivery_items)
                    .max(search_items)
                    .max(transfer_items)
                    .max(invocation_items)
                    .max(update_items),
                preferred_rows,
            );
        }
    }

    fn keep_overlay_selection_visible(&mut self, layout: &LayoutSnapshot) {
        let Some(overlay) = layout.overlay.as_ref() else {
            return;
        };
        let visible = overlay.items.len().max(1);
        if self.palette.is_some() {
            self.ensure_palette_visible(visible);
        } else if self.global_delivery.is_some() {
            self.ensure_global_delivery_visible(visible);
        } else if self.invocation_popup.is_some() {
            self.ensure_invocation_visible(usize::from(overlay.area.height.saturating_sub(3)));
        } else if self.transfer.is_some() {
            self.ensure_transfer_visible(visible);
        } else if self.search.is_some() {
            self.ensure_search_visible(visible);
        }
    }

    fn footer_summary(&self, available_width: u16) -> String {
        let count = self.visible_thought_count();
        let noun = if count == 1 { "thought" } else { "thoughts" };
        let durability = self.durability_summary();
        let inbox = self
            .screenshot_footer_state(false)
            .map_or_else(String::new, |label| format!(" · {label}"));
        let mode = match self.interaction_mode() {
            InteractionMode::Board if self.range_latched() => Some("range"),
            InteractionMode::Board => Some("board"),
            InteractionMode::Compose => None,
            InteractionMode::Edit { .. } => Some("edit"),
        };
        let complete = mode.map_or_else(
            || format!("{count} {noun} · {durability}{inbox}"),
            |mode| format!("{count} {noun} · {mode} · {durability}{inbox}"),
        );
        let compact_inbox = self
            .screenshot_footer_state(true)
            .map_or_else(String::new, |label| format!(" · {label}"));
        let compact = mode.map_or_else(
            || format!("{count} · {durability}{compact_inbox}"),
            |mode| format!("{count} · {mode} · {durability}{compact_inbox}"),
        );
        let fallback = self
            .screenshot_footer_state(true)
            .unwrap_or_else(|| format!("{count} {durability}"));
        fitting_footer_summary(complete, compact, fallback, available_width)
    }

    pub(crate) fn session_display_name(&self) -> &str {
        let session = &self.state.board.session;
        session.name.as_deref().unwrap_or_else(|| {
            session
                .last_opened_cwd
                .file_name()
                .or_else(|| session.origin_cwd.file_name())
                .and_then(|name| name.to_str())
                .unwrap_or("untitled")
        })
    }

    fn durability_summary(&self) -> &'static str {
        if matches!(self.state.durability, DurabilityState::Failed { .. }) {
            "unsaved"
        } else if self.has_pending_edit()
            || matches!(self.state.durability, DurabilityState::Pending { .. })
        {
            "saving"
        } else {
            "saved"
        }
    }
}

fn fitting_footer_summary(
    complete: String,
    compact: String,
    fallback: String,
    available_width: u16,
) -> String {
    let available = usize::from(available_width);
    if crate::ports::text_layout::terminal_cell_width(&complete) <= available {
        complete
    } else if crate::ports::text_layout::terminal_cell_width(&compact) <= available {
        compact
    } else {
        fallback
    }
}

#[cfg(test)]
mod tests {
    use super::fitting_footer_summary;

    #[test]
    fn footer_compaction_uses_exact_terminal_cell_thresholds() {
        let choose = |width| {
            fitting_footer_summary(
                "界e\u{301}👩‍💻".to_owned(),
                "界".to_owned(),
                "x".to_owned(),
                width,
            )
        };
        assert_eq!(choose(5), "界e\u{301}👩‍💻");
        assert_eq!(choose(4), "界");
        assert_eq!(choose(1), "x");
    }
}
