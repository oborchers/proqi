//! Canonical visual-row flow and semantic board scroll anchors.

use crate::{
    application::{AppState, InteractionMode},
    domain::{Thought, ThoughtId, ThoughtPresentation},
    ports::{editor::EditorSnapshot, text_layout::wrap_rows},
};

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(in crate::ui) enum ScrollAnchor {
    #[default]
    Start,
    GapBefore {
        thought_id: ThoughtId,
        row: usize,
    },
    Content {
        thought_id: ThoughtId,
        byte: usize,
    },
    Overflow(ThoughtId),
    Compose {
        byte: usize,
    },
    InsertGap,
    Insert,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::ui) enum BoardViewport {
    FollowFocus(ScrollAnchor),
    Manual(ScrollAnchor),
}

impl Default for BoardViewport {
    fn default() -> Self {
        Self::FollowFocus(ScrollAnchor::Start)
    }
}

impl BoardViewport {
    pub(in crate::ui) const fn anchor(self) -> ScrollAnchor {
        match self {
            Self::FollowFocus(anchor) | Self::Manual(anchor) => anchor,
        }
    }

    pub(in crate::ui) const fn follow_focus(self) -> Self {
        Self::FollowFocus(self.anchor())
    }

    pub(in crate::ui) const fn at(self, anchor: ScrollAnchor) -> Self {
        match self {
            Self::FollowFocus(_) => Self::FollowFocus(anchor),
            Self::Manual(_) => Self::Manual(anchor),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(in crate::ui) struct ScrollGeometry {
    pub(in crate::ui) current: ScrollAnchor,
    pub(in crate::ui) previous: Option<ScrollAnchor>,
    pub(in crate::ui) next: Option<ScrollAnchor>,
    pub(in crate::ui) maximum: ScrollAnchor,
}

#[derive(Clone, Debug)]
pub(super) struct ThoughtRows {
    pub(super) thought_id: ThoughtId,
    pub(super) index: usize,
    pub(super) gap_start: usize,
    pub(super) gap_rows: usize,
    pub(super) content_start: usize,
    pub(super) row_starts: Vec<usize>,
    pub(super) content_rows: usize,
    pub(super) natural_rows: usize,
    pub(super) overflow_row: Option<usize>,
    pub(super) end: usize,
    pub(super) presentation: ThoughtPresentation,
}

#[derive(Clone, Debug)]
pub(super) struct BoardFlow {
    pub(super) thoughts: Vec<ThoughtRows>,
    pub(super) top_padding: u16,
    pub(super) compose: Option<ComposeRows>,
    pub(super) insert_gap: Option<usize>,
    pub(super) insert_row: Option<usize>,
    total_rows: usize,
}

#[derive(Clone, Debug)]
pub(super) struct ComposeRows {
    pub(super) content_start: usize,
    pub(super) row_starts: Vec<usize>,
    pub(super) scroll_row: usize,
    pub(super) end: usize,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct ResolvedScroll {
    pub(super) offset: usize,
    pub(super) first_index: usize,
    pub(super) first_row_offset: usize,
    pub(super) max_first_index: usize,
    pub(super) geometry: ScrollGeometry,
}

struct MeasureContext<'a> {
    state: &'a AppState,
    editor: Option<&'a EditorSnapshot>,
    content_width: u16,
    board_height: u16,
    gap_rows: usize,
}

impl BoardFlow {
    pub(super) fn measure(
        state: &AppState,
        editor: Option<&EditorSnapshot>,
        content_width: u16,
        board_height: u16,
        density: crate::ui::settings::BoardDensity,
    ) -> Self {
        let live = state.board.live_thoughts();
        let roomy = usize::from(board_height)
            >= live
                .len()
                .saturating_add(live.len().saturating_sub(1).saturating_mul(3));
        let comfortable = roomy && density == crate::ui::settings::BoardDensity::Comfortable;
        let gap_rows = if comfortable { 2 } else { 1 };
        let top_padding = u16::from(comfortable && board_height >= 3 && !live.is_empty());
        let mut cursor = 0_usize;
        let mut thoughts = Vec::with_capacity(live.len());
        let context = MeasureContext {
            state,
            editor,
            content_width,
            board_height,
            gap_rows,
        };
        for (index, thought) in live.into_iter().enumerate() {
            let rows = measure_thought(&context, thought, index, cursor);
            cursor = rows.end;
            thoughts.push(rows);
        }
        let compose = if matches!(state.mode, InteractionMode::Compose) {
            editor.map(|snapshot| {
                let gap = usize::from(!thoughts.is_empty()) * gap_rows;
                let content_start = cursor.saturating_add(gap);
                let row_starts = snapshot
                    .visual_lines
                    .iter()
                    .map(|row| row.start_byte)
                    .collect::<Vec<_>>();
                let rows = row_starts.len().max(1);
                ComposeRows {
                    content_start,
                    row_starts,
                    scroll_row: snapshot.scroll_row,
                    end: content_start.saturating_add(rows),
                }
            })
        } else {
            None
        };
        if let Some(compose) = &compose {
            cursor = compose.end;
        }
        let insertion_prompt = matches!(state.mode, InteractionMode::Board)
            || (matches!(state.mode, InteractionMode::Compose) && editor.is_none());
        let insert_gap = insertion_prompt.then_some(cursor);
        cursor = cursor.saturating_add(usize::from(insertion_prompt));
        let insert_row = insertion_prompt.then_some(cursor);
        cursor = cursor.saturating_add(usize::from(insertion_prompt));
        Self {
            thoughts,
            top_padding,
            compose,
            insert_gap,
            insert_row,
            total_rows: cursor,
        }
    }

    pub(super) fn resolve(
        &self,
        viewport: BoardViewport,
        focused: Option<ThoughtId>,
        insertion_focused: bool,
        board_height: u16,
    ) -> ResolvedScroll {
        let viewport_height = board_height.saturating_sub(self.top_padding).max(1);
        let maximum = self.total_rows.saturating_sub(usize::from(viewport_height));
        let mut offset = self
            .ordinal(viewport.anchor())
            .or_else(|| {
                insertion_focused
                    .then_some(self.insert_row.or(self.insert_gap))
                    .flatten()
            })
            .or_else(|| {
                focused.and_then(|thought_id| {
                    self.thought(thought_id)
                        .map(|thought| thought.content_start)
                })
            })
            .unwrap_or(0)
            .min(maximum);
        if matches!(viewport, BoardViewport::FollowFocus(_)) {
            offset = self.follow_focus_offset(
                offset,
                maximum,
                usize::from(viewport_height),
                focused,
                insertion_focused,
            );
        }
        let current = self.anchor_at(offset);
        let previous = (offset > 0).then(|| self.anchor_at(offset - 1));
        let next = (offset < maximum).then(|| self.anchor_at(offset + 1));
        let max_anchor = self.anchor_at(maximum);
        let first = self.first_at(offset);
        let max_first = self.first_at(maximum);
        ResolvedScroll {
            offset,
            first_index: first.0,
            first_row_offset: first.1,
            max_first_index: max_first.0,
            geometry: ScrollGeometry {
                current,
                previous,
                next,
                maximum: max_anchor,
            },
        }
    }

    pub(super) fn legacy_anchor(&self, index: usize, row: usize) -> ScrollAnchor {
        let Some(thought) = self.thoughts.get(index) else {
            return ScrollAnchor::Start;
        };
        let row = row.min(thought.content_rows.saturating_sub(1));
        ScrollAnchor::Content {
            thought_id: thought.thought_id,
            byte: thought.row_starts.get(row).copied().unwrap_or(0),
        }
    }

    fn ordinal(&self, anchor: ScrollAnchor) -> Option<usize> {
        match anchor {
            ScrollAnchor::Start => Some(0),
            ScrollAnchor::GapBefore { thought_id, row } => self
                .thought(thought_id)
                .map(|thought| thought.gap_start + row.min(thought.gap_rows)),
            ScrollAnchor::Content { thought_id, byte } => self.thought(thought_id).map(|thought| {
                let row = thought
                    .row_starts
                    .iter()
                    .take(thought.content_rows)
                    .rposition(|start| *start <= byte)
                    .unwrap_or(0);
                thought.content_start + row
            }),
            ScrollAnchor::Overflow(thought_id) => self.thought(thought_id).map(|thought| {
                thought.overflow_row.unwrap_or_else(|| {
                    thought
                        .content_start
                        .saturating_add(thought.content_rows.saturating_sub(1))
                })
            }),
            ScrollAnchor::Compose { byte } => self.compose.as_ref().map(|compose| {
                let row = compose
                    .row_starts
                    .iter()
                    .rposition(|start| *start <= byte)
                    .unwrap_or(0);
                compose.content_start.saturating_add(row)
            }),
            ScrollAnchor::InsertGap => self.insert_gap,
            ScrollAnchor::Insert => self.insert_row,
        }
    }

    fn anchor_at(&self, ordinal: usize) -> ScrollAnchor {
        if self
            .thoughts
            .first()
            .is_some_and(|thought| ordinal < thought.gap_start)
        {
            return ScrollAnchor::Start;
        }
        for thought in &self.thoughts {
            if ordinal < thought.content_start && ordinal >= thought.gap_start {
                return ScrollAnchor::GapBefore {
                    thought_id: thought.thought_id,
                    row: ordinal - thought.gap_start,
                };
            }
            if ordinal >= thought.content_start
                && ordinal < thought.content_start + thought.content_rows
            {
                let row = ordinal - thought.content_start;
                return ScrollAnchor::Content {
                    thought_id: thought.thought_id,
                    byte: thought.row_starts.get(row).copied().unwrap_or(0),
                };
            }
            if thought.overflow_row == Some(ordinal) {
                return ScrollAnchor::Overflow(thought.thought_id);
            }
        }
        if let Some(compose) = &self.compose
            && ordinal >= compose.content_start
            && ordinal < compose.end
        {
            let row = ordinal.saturating_sub(compose.content_start);
            return ScrollAnchor::Compose {
                byte: compose.row_starts.get(row).copied().unwrap_or(0),
            };
        }
        if self.insert_gap == Some(ordinal) && self.insert_gap != self.insert_row {
            return ScrollAnchor::InsertGap;
        }
        if self.insert_row == Some(ordinal) {
            return ScrollAnchor::Insert;
        }
        ScrollAnchor::Start
    }

    fn first_at(&self, offset: usize) -> (usize, usize) {
        let Some(thought) = self.thoughts.iter().find(|thought| thought.end > offset) else {
            return (self.thoughts.len().saturating_sub(1), 0);
        };
        let row = offset
            .saturating_sub(thought.content_start)
            .min(thought.content_rows.saturating_sub(1));
        (thought.index, row)
    }

    fn thought(&self, thought_id: ThoughtId) -> Option<&ThoughtRows> {
        self.thoughts
            .iter()
            .find(|thought| thought.thought_id == thought_id)
    }

    fn follow_focus_offset(
        &self,
        offset: usize,
        maximum: usize,
        viewport_height: usize,
        focused: Option<ThoughtId>,
        insertion_focused: bool,
    ) -> usize {
        if insertion_focused {
            return maximum;
        }
        if let Some(compose) = &self.compose {
            return compose
                .content_start
                .saturating_add(compose.scroll_row)
                .min(maximum);
        }
        let Some(rows) = focused.and_then(|id| self.thought(id)) else {
            return offset;
        };
        let visible = offset..offset.saturating_add(viewport_height);
        if visible.contains(&rows.content_start) {
            offset
        } else {
            rows.content_start.min(maximum)
        }
    }
}

fn measure_thought(
    context: &MeasureContext<'_>,
    thought: &Thought,
    index: usize,
    cursor: usize,
) -> ThoughtRows {
    let gap_rows = usize::from(index > 0) * context.gap_rows;
    let content_start = cursor.saturating_add(gap_rows);
    let active_editor = context.editor.filter(|_| {
        matches!(context.state.mode, InteractionMode::Edit { thought_id } if thought_id == thought.id)
    });
    let row_starts = active_editor.map_or_else(
        || wrapped_row_starts(&thought.content, context.content_width),
        |snapshot| {
            snapshot
                .visual_lines
                .iter()
                .map(|row| row.start_byte)
                .collect()
        },
    );
    let natural_rows = row_starts.len().max(1);
    let cap = presentation_cap(
        thought.presentation,
        natural_rows,
        context.board_height,
        active_editor.is_some(),
    );
    let capped = natural_rows > cap;
    let content_rows = if capped {
        cap.saturating_sub(1)
    } else {
        natural_rows
    };
    let overflow_row = capped.then(|| content_start.saturating_add(content_rows));
    let end = content_start
        .saturating_add(content_rows)
        .saturating_add(usize::from(capped));
    ThoughtRows {
        thought_id: thought.id,
        index,
        gap_start: cursor,
        gap_rows,
        content_start,
        row_starts,
        content_rows,
        natural_rows,
        overflow_row,
        end,
        presentation: thought.presentation,
    }
}

fn wrapped_row_starts(content: &str, width: u16) -> Vec<usize> {
    wrap_rows(content, usize::from(width.max(1)))
        .into_iter()
        .map(|row| row.start_byte)
        .collect()
}

fn presentation_cap(
    presentation: ThoughtPresentation,
    natural_rows: usize,
    board_height: u16,
    editing: bool,
) -> usize {
    if editing || presentation == ThoughtPresentation::Expanded {
        return natural_rows;
    }
    match presentation {
        ThoughtPresentation::Collapsed => 2,
        ThoughtPresentation::Automatic => {
            usize::from(board_height.saturating_mul(2).div_ceil(3).max(3))
        }
        ThoughtPresentation::Expanded => natural_rows,
    }
    .max(1)
}
