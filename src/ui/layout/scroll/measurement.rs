//! Wrapped-row and responsive presentation measurements.

use crate::{domain::ThoughtPresentation, ports::text_layout::wrap_rows};

pub(super) fn wrapped_row_starts(content: &str, width: u16) -> Vec<usize> {
    wrap_rows(content, usize::from(width.max(1)))
        .into_iter()
        .map(|row| row.start_byte)
        .collect()
}

pub(super) fn presentation_cap(
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
