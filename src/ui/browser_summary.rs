//! Terminal-cell-bounded session summaries.

use unicode_segmentation::UnicodeSegmentation as _;

#[must_use]
pub(crate) fn summary(text: &str, limit: usize) -> String {
    let sanitized = text.replace(['\r', '\n'], " ");
    let mut cells = 0_usize;
    sanitized
        .graphemes(true)
        .take_while(|grapheme| {
            let width = unicode_width::UnicodeWidthStr::width(*grapheme);
            let fits = cells.saturating_add(width) <= limit;
            if fits {
                cells = cells.saturating_add(width);
            }
            fits
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::summary;

    #[test]
    fn budget_is_terminal_cells_not_grapheme_count() {
        assert_eq!(summary("界界界 active", 5), "界界");
        assert_eq!(summary("e\u{301}clair", 2), "e\u{301}c");
    }
}
