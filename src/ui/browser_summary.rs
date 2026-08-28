//! Terminal-cell-bounded session summaries.

#[must_use]
pub(crate) fn summary(text: &str, limit: usize) -> String {
    let sanitized = text.replace(['\r', '\n'], " ");
    crate::ports::text_layout::truncate_cells(&sanitized, limit)
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
