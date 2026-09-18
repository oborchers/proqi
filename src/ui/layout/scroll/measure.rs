//! Canonical measurement of thoughts and separators into one visual-row flow.

use crate::{
    application::{AppState, InteractionMode},
    domain::BoardItemRef,
    ui::projection::FramePresentation,
};

use super::{
    BoardFlow, ComposeRows, ItemRows, MeasureContext, SeparatorRows, ThoughtRows, measure_thought,
};

struct MeasuredItems {
    cursor: usize,
    thoughts: Vec<ThoughtRows>,
    separators: Vec<SeparatorRows>,
    items: Vec<ItemRows>,
    previous_was_thought: bool,
}

impl BoardFlow {
    pub(in crate::ui::layout) fn measure(
        state: &AppState,
        presentation: &FramePresentation,
        content_width: u16,
        board_height: u16,
        density: crate::ui::settings::BoardDensity,
    ) -> Self {
        let live = state.board.live_items();
        let density = density.resolve(board_height);
        let comfortable = density == crate::ui::settings::BoardDensity::Comfortable;
        let gap_rows = if comfortable { 2 } else { 1 };
        let top_padding = u16::from(
            comfortable
                && board_height >= 3
                && live
                    .first()
                    .and_then(|item| item.thought())
                    .is_some_and(|thought| thought.name.is_none()),
        );
        let context = MeasureContext {
            state,
            presentation,
            content_width,
            board_height,
            gap_rows,
        };
        let measured = measure_items(&context, &live, comfortable);
        let mut cursor = measured.cursor;
        let compose = if matches!(state.mode, InteractionMode::Compose) {
            presentation.editor_snapshot().map(|snapshot| {
                let gap = usize::from(measured.previous_was_thought) * gap_rows;
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
            || (matches!(state.mode, InteractionMode::Compose)
                && presentation.editor_snapshot().is_none());
        let insert_gap = insertion_prompt.then_some(cursor);
        cursor = cursor.saturating_add(usize::from(insertion_prompt));
        let insert_row = insertion_prompt.then_some(cursor);
        cursor = cursor.saturating_add(usize::from(insertion_prompt));
        Self {
            thoughts: measured.thoughts,
            separators: measured.separators,
            items: measured.items,
            top_padding,
            compose,
            insert_gap,
            insert_row,
            total_rows: cursor,
            density,
        }
    }
}

fn measure_items(
    context: &MeasureContext<'_>,
    live: &[BoardItemRef<'_>],
    comfortable: bool,
) -> MeasuredItems {
    let mut measured = MeasuredItems {
        cursor: 0,
        thoughts: Vec::new(),
        separators: Vec::new(),
        items: Vec::with_capacity(live.len()),
        previous_was_thought: false,
    };
    for (index, item) in live.iter().copied().enumerate() {
        match item {
            BoardItemRef::Thought(thought) => {
                push_thought(context, &mut measured, thought.id, index);
            }
            BoardItemRef::Separator(separator) => {
                push_separator(&mut measured, separator.id, index, comfortable);
            }
        }
    }
    measured
}

fn push_thought(
    context: &MeasureContext<'_>,
    measured: &mut MeasuredItems,
    thought_id: crate::domain::ThoughtId,
    index: usize,
) {
    let Some(presented) = context.presentation.thought(thought_id) else {
        return;
    };
    let rows = measure_thought(
        context,
        presented,
        index,
        measured.cursor,
        measured.previous_was_thought,
    );
    measured.cursor = rows.end;
    measured
        .items
        .push(ItemRows::Thought(measured.thoughts.len()));
    measured.thoughts.push(rows);
    measured.previous_was_thought = true;
}

fn push_separator(
    measured: &mut MeasuredItems,
    separator_id: crate::domain::SeparatorId,
    index: usize,
    comfortable: bool,
) {
    let height = if comfortable { 3 } else { 1 };
    let line_row = measured.cursor.saturating_add(usize::from(comfortable));
    let rows = SeparatorRows {
        separator_id,
        index,
        start: measured.cursor,
        line: line_row,
        end: measured.cursor.saturating_add(height),
    };
    measured.cursor = rows.end;
    measured
        .items
        .push(ItemRows::Separator(measured.separators.len()));
    measured.separators.push(rows);
    measured.previous_was_thought = false;
}
