//! Equivalence between the batch scanner and the canonical editing recognizer.

use crate::ports::text_layout::logical_lines;

use super::super::{indentation_marker_at, recognized_prefix_lengths};

#[test]
fn batch_recognition_matches_canonical_editing_across_structural_contexts() {
    for (content, width) in [
        ("- one\n  continuation\n    - nested\n- two", 2),
        ("- one\n\t- tab nested\nplain\n    - detached", 4),
        ("---\n- after thematic\n***\n1. ordered", 2),
        ("```md\n- fenced\n```\n- visible", 2),
        ("- before\n\n    - after blank", 2),
        ("- parent\n  - two spaces\n    - four spaces", 4),
    ] {
        let lines = logical_lines(content);
        let expected = lines
            .iter()
            .enumerate()
            .map(|(index, line)| {
                indentation_marker_at(content, &lines, index, width).map(|marker| {
                    let line_text = &content[line.start..line.content_end];
                    line_text.len().saturating_sub(marker.content.len())
                })
            })
            .collect::<Vec<_>>();
        assert_eq!(
            recognized_prefix_lengths(content, &lines, width),
            expected,
            "content {content:?}, width {width}"
        );
    }
}
