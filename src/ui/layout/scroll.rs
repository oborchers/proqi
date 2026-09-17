//! Canonical visual-row flow and semantic board scroll anchors.

use crate::{
    application::{AppState, InteractionMode},
    domain::{BoardItemId, SeparatorId, ThoughtId, ThoughtPresentation},
    ports::text_layout::wrap_rows,
    ui::projection::{FramePresentation, PresentedThought},
};

mod anchor;
mod focus;
mod measure;
#[cfg(test)]
mod tests;

pub(in crate::ui) use anchor::ContentAnchor;
use anchor::{content_row_anchors, content_row_for_anchor};

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
        position: ContentAnchor,
    },
    Overflow(ThoughtId),
    Separator(SeparatorId),
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
    focused_previous: Option<ScrollAnchor>,
    focused_next: Option<ScrollAnchor>,
    pub(in crate::ui) maximum: ScrollAnchor,
    pub(in crate::ui) maximum_offset: usize,
}

#[derive(Clone, Debug)]
pub(super) struct ThoughtRows {
    pub(super) thought_id: ThoughtId,
    pub(super) index: usize,
    pub(super) gap_start: usize,
    pub(super) gap_rows: usize,
    pub(super) content_start: usize,
    row_anchors: Vec<ContentAnchor>,
    pub(super) content_rows: usize,
    pub(super) natural_rows: usize,
    pub(super) overflow_row: Option<usize>,
    pub(super) end: usize,
    pub(super) presentation: ThoughtPresentation,
    editing: bool,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct SeparatorRows {
    pub(super) separator_id: SeparatorId,
    pub(super) index: usize,
    pub(super) start: usize,
    pub(super) line: usize,
    pub(super) end: usize,
}

#[derive(Clone, Copy, Debug)]
enum ItemRows {
    Thought(usize),
    Separator(usize),
}

#[derive(Clone, Debug)]
pub(super) struct BoardFlow {
    pub(super) thoughts: Vec<ThoughtRows>,
    pub(super) separators: Vec<SeparatorRows>,
    items: Vec<ItemRows>,
    pub(super) top_padding: u16,
    pub(super) compose: Option<ComposeRows>,
    pub(super) insert_gap: Option<usize>,
    pub(super) insert_row: Option<usize>,
    pub(super) total_rows: usize,
    pub(super) density: crate::ui::settings::BoardDensity,
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
    presentation: &'a FramePresentation,
    content_width: u16,
    board_height: u16,
    gap_rows: usize,
}

impl BoardFlow {
    pub(super) fn resolve(
        &self,
        viewport: BoardViewport,
        focused: Option<BoardItemId>,
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
            .or_else(|| focused.and_then(|item_id| self.item_start(item_id)))
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
        let (focused_previous, focused_next) =
            focus::neighbors(self, offset, usize::from(viewport_height), maximum, focused);
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
                focused_previous,
                focused_next,
                maximum: max_anchor,
                maximum_offset: maximum,
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
            position: thought
                .row_anchors
                .get(row)
                .copied()
                .unwrap_or(ContentAnchor::Canonical(0)),
        }
    }

    fn ordinal(&self, anchor: ScrollAnchor) -> Option<usize> {
        match anchor {
            ScrollAnchor::Start => Some(0),
            ScrollAnchor::GapBefore { thought_id, row } => self
                .thought(thought_id)
                .map(|thought| thought.gap_start + row.min(thought.gap_rows)),
            ScrollAnchor::Content {
                thought_id,
                position,
            } => self
                .thought(thought_id)
                .map(|thought| thought.content_start + content_row_for_anchor(thought, position)),
            ScrollAnchor::Overflow(thought_id) => self.thought(thought_id).map(|thought| {
                thought.overflow_row.unwrap_or_else(|| {
                    thought
                        .content_start
                        .saturating_add(thought.content_rows.saturating_sub(1))
                })
            }),
            ScrollAnchor::Separator(separator_id) => {
                self.separator(separator_id).map(|separator| separator.line)
            }
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
                    position: thought
                        .row_anchors
                        .get(row)
                        .copied()
                        .unwrap_or(ContentAnchor::Canonical(0)),
                };
            }
            if thought.overflow_row == Some(ordinal) {
                return ScrollAnchor::Overflow(thought.thought_id);
            }
        }
        for separator in &self.separators {
            if ordinal >= separator.start && ordinal < separator.end {
                return ScrollAnchor::Separator(separator.separator_id);
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
        let Some(item) = self
            .items
            .iter()
            .find(|item| self.item_end(**item) > offset)
        else {
            return (self.items.len().saturating_sub(1), 0);
        };
        match item {
            ItemRows::Thought(index) => {
                let thought = &self.thoughts[*index];
                let row = offset
                    .saturating_sub(thought.content_start)
                    .min(thought.content_rows.saturating_sub(1));
                (thought.index, row)
            }
            ItemRows::Separator(index) => (self.separators[*index].index, 0),
        }
    }

    fn thought(&self, thought_id: ThoughtId) -> Option<&ThoughtRows> {
        self.thoughts
            .iter()
            .find(|thought| thought.thought_id == thought_id)
    }

    fn separator(&self, separator_id: SeparatorId) -> Option<&SeparatorRows> {
        self.separators
            .iter()
            .find(|separator| separator.separator_id == separator_id)
    }

    fn item_start(&self, item_id: BoardItemId) -> Option<usize> {
        match item_id {
            BoardItemId::Thought(id) => self.thought(id).map(|item| item.content_start),
            BoardItemId::Separator(id) => self.separator(id).map(|item| item.start),
        }
    }

    fn item_end(&self, item: ItemRows) -> usize {
        match item {
            ItemRows::Thought(index) => self.thoughts[index].end,
            ItemRows::Separator(index) => self.separators[index].end,
        }
    }

    fn follow_focus_offset(
        &self,
        offset: usize,
        maximum: usize,
        viewport_height: usize,
        focused: Option<BoardItemId>,
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
        let Some(focused) = focused else {
            return offset;
        };
        if let BoardItemId::Separator(separator_id) = focused {
            let Some(separator) = self.separator(separator_id) else {
                return offset;
            };
            let visible = offset..offset.saturating_add(viewport_height);
            return if visible.contains(&separator.line) {
                offset
            } else {
                separator.line.min(maximum)
            };
        }
        let BoardItemId::Thought(focused) = focused else {
            return offset;
        };
        let Some(rows) = self.thought(focused) else {
            return offset;
        };
        if rows.editing {
            // Reveal the full editor allocation before clipping establishes its
            // internal viewport. Its previous visible height is not a cap.
            let end = rows.content_start + rows.content_rows.min(viewport_height);
            return offset
                .max(end.saturating_sub(viewport_height))
                .min(rows.content_start)
                .min(maximum);
        }
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
    thought: &PresentedThought,
    index: usize,
    cursor: usize,
    previous_was_thought: bool,
) -> ThoughtRows {
    let gap_rows = usize::from(previous_was_thought) * context.gap_rows;
    let content_start = cursor.saturating_add(gap_rows);
    let active_editor = context.presentation.editor_snapshot().filter(|_| {
        matches!(context.state.mode, InteractionMode::Edit { thought_id } if thought_id == thought.thought_id)
    });
    let row_starts = active_editor.map_or_else(
        || wrapped_row_starts(&thought.presentation.content, context.content_width),
        |snapshot| {
            snapshot
                .visual_lines
                .iter()
                .map(|row| row.start_byte)
                .collect()
        },
    );
    let row_anchors = content_row_anchors(thought, &row_starts);
    let natural_rows = row_starts.len().max(1);
    let cap = presentation_cap(
        thought.preference,
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
        thought_id: thought.thought_id,
        index,
        gap_start: cursor,
        gap_rows,
        content_start,
        row_anchors,
        content_rows,
        natural_rows,
        overflow_row,
        end,
        presentation: thought.preference,
        editing: active_editor.is_some(),
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
