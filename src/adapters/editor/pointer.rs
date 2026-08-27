//! Unicode-aware pointer selection state and boundary resolution.

use crate::{
    domain::TextPosition,
    ports::{
        editor::SelectionGranularity,
        text_layout::{byte_for_position, logical_lines, position_for_byte},
    },
};

use super::{RopeEditor, text};

#[derive(Clone, Copy)]
pub(super) struct PointerSelection {
    base_start: usize,
    base_end: usize,
    granularity: SelectionGranularity,
}

impl RopeEditor {
    fn selection_range_at(
        content: &str,
        byte: usize,
        granularity: SelectionGranularity,
    ) -> (usize, usize) {
        match granularity {
            SelectionGranularity::Grapheme => (byte, byte),
            SelectionGranularity::Word => text::word_range(content, byte).unwrap_or_else(|| {
                let lines = logical_lines(content);
                let line = lines[position_for_byte(content, byte).line];
                if byte == line.content_end {
                    (byte, byte)
                } else {
                    text::grapheme_range(content, byte)
                }
            }),
            SelectionGranularity::LogicalLine => {
                let lines = logical_lines(content);
                let line = lines[position_for_byte(content, byte).line];
                (line.start, line.end)
            }
        }
    }

    pub(super) fn begin_pointer_selection(
        &mut self,
        position: TextPosition,
        granularity: SelectionGranularity,
        extend_selection: bool,
    ) -> bool {
        let content = self.content();
        let byte = byte_for_position(&content, position);
        let target = Self::selection_range_at(&content, byte, granularity);
        let base = if extend_selection {
            let anchor = self
                .state
                .selection_anchor_byte
                .unwrap_or(self.state.cursor_byte);
            (anchor, anchor)
        } else {
            target
        };
        self.pointer_selection = Some(PointerSelection {
            base_start: base.0,
            base_end: base.1,
            granularity,
        });
        self.extend_pointer_selection(position);
        false
    }

    pub(super) fn extend_pointer_selection(&mut self, position: TextPosition) -> bool {
        let Some(pointer) = self.pointer_selection else {
            return false;
        };
        let content = self.content();
        let byte = byte_for_position(&content, position);
        let target = Self::selection_range_at(&content, byte, pointer.granularity);
        let (anchor, cursor) = if target.1 <= pointer.base_start {
            (pointer.base_end, target.0)
        } else if target.0 >= pointer.base_end {
            (pointer.base_start, target.1)
        } else {
            (pointer.base_start, pointer.base_end)
        };
        self.state.selection_anchor_byte = Some(anchor);
        self.state.cursor_byte = cursor;
        self.ensure_cursor_visible();
        false
    }

    pub(super) fn end_pointer_selection(&mut self) -> bool {
        self.pointer_selection = None;
        false
    }
}
