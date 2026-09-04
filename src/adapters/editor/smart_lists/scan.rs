//! One-pass structural recognition for whole-document list consumers.

#[cfg(test)]
mod tests;

use crate::ports::structured_text::{FenceState, is_thematic_break, parse_list_marker};
use crate::ports::text_layout::LogicalLine;

use super::whitespace_suffix_range;

pub(in crate::adapters::editor) fn recognized_prefix_lengths(
    content: &str,
    lines: &[LogicalLine],
    width: u8,
) -> Vec<Option<usize>> {
    let mut open_fence = FenceState::closed();
    let mut adjacent_list_context = false;
    let mut prefixes = Vec::with_capacity(lines.len());
    for line in lines {
        let line_text = &content[line.start..line.content_end];
        let parsed = parse_list_marker(line_text);
        let top_level = parsed.filter(|marker| {
            !open_fence.is_open()
                && !is_thematic_break(line_text)
                && marker.indentation_columns() <= 3
        });
        let recognized = top_level.or_else(|| {
            let marker = parsed?;
            let indentation_end = line.start + marker.indentation().len();
            let has_indentation_unit = marker.indentation().contains('\t')
                || whitespace_suffix_range(content, line.start, indentation_end, width).is_some();
            (!open_fence.is_open()
                && !is_thematic_break(line_text)
                && has_indentation_unit
                && adjacent_list_context)
                .then_some(marker)
        });
        prefixes
            .push(recognized.map(|marker| line_text.len().saturating_sub(marker.content().len())));
        adjacent_list_context = if line_text.trim_matches([' ', '\t']).is_empty() {
            false
        } else if top_level.is_some() {
            true
        } else if line_text.starts_with([' ', '\t']) {
            adjacent_list_context
        } else {
            false
        };
        open_fence.update(line_text);
    }
    prefixes
}
