//! Responsive release-highlight overlay with durable explicit acknowledgement.

use crate::{
    application::{Effect, ReleaseHighlightPresentation, UpdateIntent},
    domain::{ReleaseHighlightAnnouncement, ReleaseHighlightGroup},
    ui::{HitTarget, ListNavigation, PointerButton, PointerKind, UiInput, UiKey},
};

use super::BoardApp;

pub(super) struct ReleaseHighlightsOverlay {
    groups: Vec<ReleaseHighlightGroup>,
    source: ReleaseHighlightsSource,
    scroll: usize,
    input_boundary: u64,
    armed: bool,
    acknowledgement_pending: bool,
}

enum ReleaseHighlightsSource {
    Automatic(ReleaseHighlightAnnouncement),
    Manual,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::ui) enum ReleaseHighlightRow {
    Version(String),
    Bullet(String),
    Continuation(String),
    Spacer,
}

pub(in crate::ui) struct ReleaseHighlightsView {
    pub(in crate::ui) title: String,
    pub(in crate::ui) rows: Vec<ReleaseHighlightRow>,
    pub(in crate::ui) scroll: usize,
}

impl BoardApp {
    pub(crate) fn install_release_highlights(
        &mut self,
        installed: Option<ReleaseHighlightGroup>,
        automatic: Option<ReleaseHighlightPresentation>,
        input_boundary: u64,
    ) {
        self.installed_highlights = installed;
        let Some(automatic) = automatic else {
            return;
        };
        self.close_for_release_highlights();
        self.release_highlights = Some(ReleaseHighlightsOverlay {
            groups: automatic.groups,
            source: ReleaseHighlightsSource::Automatic(automatic.announcement),
            scroll: 0,
            input_boundary,
            armed: false,
            acknowledgement_pending: false,
        });
        self.layout = None;
    }

    pub(super) fn open_installed_release_highlights(&mut self) -> Vec<Effect> {
        let Some(group) = self.installed_highlights.clone() else {
            self.set_warning("What's new is unavailable for this Proqi installation.");
            return Vec::new();
        };
        self.close_for_release_highlights();
        self.release_highlights = Some(ReleaseHighlightsOverlay {
            groups: vec![group],
            source: ReleaseHighlightsSource::Manual,
            scroll: 0,
            input_boundary: 0,
            armed: true,
            acknowledgement_pending: false,
        });
        self.layout = None;
        Vec::new()
    }

    fn close_for_release_highlights(&mut self) {
        self.deactivate_range_latch();
        self.help = false;
        self.palette = None;
        self.invocation_popup = None;
        self.search = None;
        self.rename = None;
        self.transfer = None;
    }

    pub(crate) fn arm_release_highlights(&mut self, input_boundary: u64) {
        if let Some(highlights) = &mut self.release_highlights {
            if highlights.armed {
                return;
            }
            highlights.input_boundary = highlights.input_boundary.max(input_boundary);
            highlights.armed = true;
        }
    }

    pub(crate) fn note_release_highlights_rendered(&mut self, rendered: bool, input_boundary: u64) {
        if !rendered {
            if let Some(highlights) = &mut self.release_highlights
                && matches!(&highlights.source, ReleaseHighlightsSource::Automatic(_))
            {
                highlights.input_boundary = highlights.input_boundary.max(input_boundary);
                highlights.armed = false;
            }
            return;
        }
        self.arm_release_highlights(input_boundary);
    }

    pub(crate) fn accept_release_highlights_input(&self, sequence: u64) -> bool {
        self.release_highlights.as_ref().is_none_or(|highlights| {
            highlights.armed && (sequence == 0 || sequence > highlights.input_boundary)
        })
    }

    pub(super) fn handle_release_highlights_input(&mut self, input: &UiInput) -> Vec<Effect> {
        match input {
            UiInput::Key(UiKey::Escape) => return self.dismiss_release_highlights(),
            UiInput::Key(key) if key.list_navigation() == Some(ListNavigation::Previous) => {
                self.scroll_release_highlights(-1);
            }
            UiInput::Key(key) if key.list_navigation() == Some(ListNavigation::Next) => {
                self.scroll_release_highlights(1);
            }
            UiInput::Pointer(pointer) => match pointer.kind {
                PointerKind::ScrollUp => self.scroll_release_highlights(-1),
                PointerKind::ScrollDown => self.scroll_release_highlights(1),
                PointerKind::Down(PointerButton::Left)
                    if self
                        .layout
                        .as_ref()
                        .and_then(|layout| layout.hit_test(pointer.column, pointer.row))
                        == Some(HitTarget::CloseOverlay) =>
                {
                    return self.dismiss_release_highlights();
                }
                PointerKind::Down(_)
                | PointerKind::Up(_)
                | PointerKind::Drag(_)
                | PointerKind::Move => {}
            },
            UiInput::Resize { .. } => {
                self.layout = None;
                self.hovered = None;
            }
            UiInput::HostFocusGained
            | UiInput::HostFocusLost
            | UiInput::Key(_)
            | UiInput::Paste(_)
            | UiInput::PasteAnnotated(_) => {}
        }
        Vec::new()
    }

    fn scroll_release_highlights(&mut self, delta: isize) {
        let maximum = self.release_highlights_max_scroll();
        if let Some(highlights) = &mut self.release_highlights {
            highlights.scroll = highlights.scroll.saturating_add_signed(delta).min(maximum);
        }
    }

    fn dismiss_release_highlights(&mut self) -> Vec<Effect> {
        let Some(highlights) = &mut self.release_highlights else {
            return Vec::new();
        };
        match &highlights.source {
            ReleaseHighlightsSource::Manual => {
                self.release_highlights = None;
                self.layout = None;
                Vec::new()
            }
            ReleaseHighlightsSource::Automatic(announcement)
                if !highlights.acknowledgement_pending =>
            {
                highlights.acknowledgement_pending = true;
                vec![Effect::Update(UpdateIntent::AcknowledgeReleaseHighlights(
                    announcement.clone(),
                ))]
            }
            ReleaseHighlightsSource::Automatic(_) => Vec::new(),
        }
    }

    pub(crate) fn complete_release_highlights_acknowledgement(&mut self, succeeded: bool) {
        if succeeded {
            self.release_highlights = None;
            self.layout = None;
        } else if let Some(highlights) = &mut self.release_highlights {
            highlights.acknowledgement_pending = false;
            self.set_error("Proqi could not save the What's new acknowledgement.");
        }
    }

    pub(in crate::ui) fn release_highlights_view(
        &self,
        width: u16,
        height: u16,
    ) -> Option<ReleaseHighlightsView> {
        let highlights = self.release_highlights.as_ref()?;
        let rows = project_rows(&highlights.groups, width);
        let capacity = usize::from(height);
        let maximum = rows.len().saturating_sub(capacity);
        Some(ReleaseHighlightsView {
            title: title(&highlights.groups),
            rows,
            scroll: highlights.scroll.min(maximum),
        })
    }

    pub(super) fn release_highlights_row_count(&self, width: u16) -> usize {
        self.release_highlights.as_ref().map_or(0, |highlights| {
            project_rows(&highlights.groups, width).len()
        })
    }

    fn release_highlights_max_scroll(&self) -> usize {
        let Some(area) = self
            .layout
            .as_ref()
            .and_then(|layout| layout.overlay.as_ref())
            .map(|overlay| overlay.area)
        else {
            return 0;
        };
        self.release_highlights_row_count(area.width.saturating_sub(2))
            .saturating_sub(usize::from(area.height.saturating_sub(2)))
    }

    pub(super) fn clamp_release_highlights_scroll(&mut self) {
        let maximum = self.release_highlights_max_scroll();
        if let Some(highlights) = &mut self.release_highlights {
            highlights.scroll = highlights.scroll.min(maximum);
        }
    }
}

fn title(groups: &[ReleaseHighlightGroup]) -> String {
    groups.last().map_or_else(
        || " what's new in Proqi ".to_owned(),
        |group| format!(" what's new in Proqi {} ", group.version()),
    )
}

fn project_rows(groups: &[ReleaseHighlightGroup], width: u16) -> Vec<ReleaseHighlightRow> {
    let mut output = Vec::new();
    let grouped = groups.len() > 1;
    for (group_index, group) in groups.iter().enumerate() {
        if group_index > 0 {
            output.push(ReleaseHighlightRow::Spacer);
        }
        if grouped {
            output.push(ReleaseHighlightRow::Version(format!(
                "Proqi {}",
                group.version()
            )));
        }
        for highlight in group.highlights() {
            let mut wrapped = crate::ports::text_layout::wrap_rows(
                highlight,
                usize::from(width.saturating_sub(2).max(1)),
            );
            if wrapped.len() > 1 && wrapped.last().is_some_and(|row| row.visual.text.is_empty()) {
                wrapped.pop();
            }
            output.extend(wrapped.into_iter().enumerate().map(project_wrapped_row));
        }
    }
    output
}

fn project_wrapped_row(
    (row_index, row): (usize, crate::ports::text_layout::WrappedRow),
) -> ReleaseHighlightRow {
    if row_index == 0 {
        ReleaseHighlightRow::Bullet(row.visual.text)
    } else {
        ReleaseHighlightRow::Continuation(row.visual.text)
    }
}

#[cfg(test)]
mod tests;
