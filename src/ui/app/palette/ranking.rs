//! Deterministic Unicode command-label ranking.

use unicode_normalization::UnicodeNormalization as _;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) struct MatchRank {
    class: u8,
    gaps: usize,
    span: usize,
}

pub(super) fn rank(label: &str, query: &str) -> Option<MatchRank> {
    let label = normalize(label);
    let query = normalize(query);
    if query.is_empty() {
        return None;
    }
    if label == query {
        return Some(simple(0));
    }
    let words = tokens(&label);
    if words.iter().any(|word| word.starts_with(&query)) {
        return Some(simple(1));
    }
    let query_tokens = tokens(&query);
    if !query_tokens.is_empty() && ordered_token_prefix(&words, &query_tokens) {
        return Some(simple(2));
    }
    if let Some((gaps, span)) = ordered_fuzzy(&label, &query)
        && !label.contains(&query)
    {
        return Some(MatchRank {
            class: 3,
            gaps,
            span,
        });
    }
    label.contains(&query).then(|| simple(4))
}

const fn simple(class: u8) -> MatchRank {
    MatchRank {
        class,
        gaps: 0,
        span: 0,
    }
}

fn normalize(value: &str) -> String {
    value
        .nfkc()
        .flat_map(char::to_lowercase)
        .collect::<String>()
        .nfkc()
        .collect()
}

fn tokens(value: &str) -> Vec<&str> {
    value
        .split(|character: char| !character.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .collect()
}

fn ordered_token_prefix(words: &[&str], query: &[&str]) -> bool {
    let mut next = 0;
    for token in query {
        let Some(offset) = words[next..]
            .iter()
            .position(|word| word.starts_with(token))
        else {
            return false;
        };
        next = next.saturating_add(offset).saturating_add(1);
    }
    true
}

fn ordered_fuzzy(label: &str, query: &str) -> Option<(usize, usize)> {
    let label = label.chars().collect::<Vec<_>>();
    let query = query.chars().collect::<Vec<_>>();
    let mut positions = Vec::with_capacity(query.len());
    let mut next = 0;
    for wanted in query {
        let offset = label
            .get(next..)?
            .iter()
            .position(|candidate| *candidate == wanted)?;
        let position = next.saturating_add(offset);
        positions.push(position);
        next = position.saturating_add(1);
    }
    let first = *positions.first()?;
    let last = *positions.last()?;
    let span = last.saturating_sub(first).saturating_add(1);
    Some((span.saturating_sub(positions.len()), span))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranking_is_unicode_normalized_and_follows_the_closed_class_order() {
        let exact = rank("Clean up spacing", "clean up spacing").expect("exact");
        let word = rank("Insert discovered invocation", "disc").expect("word");
        let tokens = rank("Send to another Proqi session", "se pro").expect("tokens");
        let fuzzy = rank("Refresh attachments", "rfat").expect("fuzzy");
        let substring = rank("Rename session", "name").expect("substring");
        assert!(exact < word && word < tokens && tokens < fuzzy && fuzzy < substring);
        assert_eq!(
            rank("Résumé session", "re\u{301}sume\u{301}"),
            rank("Résumé session", "résumé"),
        );
    }
}
