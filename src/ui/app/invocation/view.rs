//! Structured invocation picker projections.

#[derive(Clone)]
pub(super) struct Choice {
    pub(super) token: String,
    pub(super) insertion: String,
    pub(super) annotation_display: Option<String>,
    pub(super) separate_from_prefix: bool,
    pub(super) qualifier: String,
    pub(super) qualifier_fallbacks: Vec<String>,
    pub(super) group: Option<String>,
    pub(super) rank: super::matcher::MatchRank,
}

/// One invocation result rendered as a responsive two-field row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::ui) struct InvocationChoiceView {
    /// Bounded primary text shown for the result.
    pub(in crate::ui) token: String,
    /// Scope or compact live topology and harness context.
    pub(in crate::ui) qualifier: String,
    /// Progressively quieter descriptions used when the full qualifier does not fit.
    pub(in crate::ui) qualifier_fallbacks: Vec<String>,
    /// Structural group rendered immediately above the first visible matching choice.
    pub(in crate::ui) group: Option<String>,
}

pub(super) fn scroll_for_selection(
    choices: &[Choice],
    selected: usize,
    current_scroll: usize,
    row_budget: usize,
) -> usize {
    let mut scroll = current_scroll.min(selected);
    let row_budget = row_budget.max(1);
    while scroll < selected && rows_through_selection(choices, scroll, selected) > row_budget {
        scroll = scroll.saturating_add(1);
    }
    scroll
}

fn rows_through_selection(choices: &[Choice], start: usize, selected: usize) -> usize {
    let mut previous_group = None;
    choices
        .iter()
        .skip(start)
        .take(selected.saturating_sub(start).saturating_add(1))
        .map(|choice| {
            let heading = choice.group.as_deref().is_some_and(|group| {
                let changed = previous_group != Some(group);
                previous_group = Some(group);
                changed
            });
            1 + usize::from(heading)
        })
        .sum()
}
