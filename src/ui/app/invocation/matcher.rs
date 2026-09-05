//! Deterministic fuzzy ranking for invocation-picker search fields.

use unicode_normalization::UnicodeNormalization as _;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum MatchClass {
    Empty,
    Exact,
    Prefix,
    Contiguous,
    Fuzzy,
}

/// One best-first rank. Equal values deliberately defer to the caller's
/// canonical discovery order.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) struct MatchRank {
    field: SearchField,
    class: MatchClass,
    boundary_misses: usize,
    runs: usize,
    span: usize,
    gaps: usize,
    start: usize,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum SearchField {
    Token,
    Secondary,
}

#[derive(Clone, Copy)]
struct Path {
    boundaries: usize,
    runs: usize,
    start: usize,
}

/// Rank a canonical invocation token. A leading query sigil is an exact
/// namespace filter, while an empty manual query retains canonical ordering.
pub(super) fn token(candidate: &str, query: &str) -> Option<MatchRank> {
    rank(candidate, query, SearchField::Token, true)
}

/// Rank subordinate manual-picker metadata. Explicit sigil queries never
/// search prose or location fields.
pub(super) fn secondary(candidate: &str, query: &str) -> Option<MatchRank> {
    (!query_starts_with_sigil(query))
        .then(|| rank(candidate, query, SearchField::Secondary, false))
        .flatten()
}

fn rank(
    candidate: &str,
    query: &str,
    field: SearchField,
    enforce_sigil: bool,
) -> Option<MatchRank> {
    record_rank_call();
    let candidate = normalize(candidate);
    let query = normalize(query);
    if enforce_sigil && incompatible_sigils(&candidate, &query) {
        return None;
    }
    if query.is_empty() {
        return Some(simple_rank(field, MatchClass::Empty));
    }
    if candidate == query {
        return Some(simple_rank(field, MatchClass::Exact));
    }
    if candidate.starts_with(&query) {
        return Some(simple_rank(field, MatchClass::Prefix));
    }
    let (candidate, query) = without_common_sigil(&candidate, &query);
    let candidate = candidate.chars().collect::<Vec<_>>();
    let query = query.chars().collect::<Vec<_>>();
    if let Some(contiguous) = contiguous_rank(&candidate, &query, field) {
        return Some(contiguous);
    }
    fuzzy_rank(&candidate, &query, field)
}

fn contiguous_rank(candidate: &[char], query: &[char], field: SearchField) -> Option<MatchRank> {
    candidate
        .windows(query.len())
        .enumerate()
        .filter(|(_, window)| *window == query)
        .map(|(start, _)| MatchRank {
            field,
            class: MatchClass::Contiguous,
            boundary_misses: usize::from(!is_boundary(candidate, start)),
            runs: 1,
            span: query.len(),
            gaps: 0,
            start,
        })
        .min()
}

fn without_common_sigil<'a>(candidate: &'a str, query: &'a str) -> (&'a str, &'a str) {
    let Some(sigil) = query
        .chars()
        .next()
        .filter(|character| is_sigil(*character))
    else {
        return (candidate, query);
    };
    (
        candidate.strip_prefix(sigil).unwrap_or(candidate),
        query.strip_prefix(sigil).unwrap_or(query),
    )
}

const fn simple_rank(field: SearchField, class: MatchClass) -> MatchRank {
    MatchRank {
        field,
        class,
        boundary_misses: 0,
        runs: 0,
        span: 0,
        gaps: 0,
        start: 0,
    }
}

fn fuzzy_rank(candidate: &[char], query: &[char], field: SearchField) -> Option<MatchRank> {
    if query.len() > candidate.len() {
        return None;
    }
    let mut previous = vec![None; candidate.len()];
    for (query_index, query_character) in query.iter().enumerate() {
        previous = fuzzy_row(candidate, *query_character, query_index, &previous);
    }
    let matched_alphanumeric = query
        .iter()
        .filter(|character| character.is_alphanumeric())
        .count();
    previous
        .iter()
        .enumerate()
        .filter_map(|(end, path)| {
            path.map(|path| fuzzy_path_rank(path, end, query.len(), matched_alphanumeric, field))
        })
        .min()
}

fn fuzzy_row(
    candidate: &[char],
    query_character: char,
    query_index: usize,
    previous: &[Option<Path>],
) -> Vec<Option<Path>> {
    let mut row = vec![None; candidate.len()];
    let mut best_non_adjacent = None;
    for (position, candidate_character) in candidate.iter().enumerate() {
        if position >= 2 {
            best_non_adjacent = better_path(best_non_adjacent, previous[position - 2]);
        }
        if candidate_character != &query_character {
            continue;
        }
        let boundary = usize::from(is_boundary(candidate, position));
        row[position] = if query_index == 0 {
            Some(Path {
                boundaries: boundary,
                runs: 1,
                start: position,
            })
        } else {
            let adjacent = position
                .checked_sub(1)
                .and_then(|index| previous[index])
                .map(|path| Path {
                    boundaries: path.boundaries.saturating_add(boundary),
                    ..path
                });
            let separated = best_non_adjacent.map(|path: Path| Path {
                boundaries: path.boundaries.saturating_add(boundary),
                runs: path.runs.saturating_add(1),
                ..path
            });
            better_path(adjacent, separated)
        };
    }
    row
}

fn better_path(left: Option<Path>, right: Option<Path>) -> Option<Path> {
    match (left, right) {
        (Some(left), Some(right)) => Some(if continuation_key(left) <= continuation_key(right) {
            left
        } else {
            right
        }),
        (Some(path), None) | (None, Some(path)) => Some(path),
        (None, None) => None,
    }
}

const fn continuation_key(path: Path) -> (usize, usize, std::cmp::Reverse<usize>) {
    (
        usize::MAX - path.boundaries,
        path.runs,
        std::cmp::Reverse(path.start),
    )
}

fn fuzzy_path_rank(
    path: Path,
    end: usize,
    query_len: usize,
    matched_alphanumeric: usize,
    field: SearchField,
) -> MatchRank {
    let span = end.saturating_sub(path.start).saturating_add(1);
    MatchRank {
        field,
        class: MatchClass::Fuzzy,
        boundary_misses: matched_alphanumeric.saturating_sub(path.boundaries),
        runs: path.runs,
        span,
        gaps: span.saturating_sub(query_len),
        start: path.start,
    }
}

fn normalize(value: &str) -> String {
    let compatible = value.nfkc().collect::<String>();
    compatible
        .chars()
        .flat_map(char::to_lowercase)
        .collect::<String>()
        .nfkc()
        .collect()
}

#[cfg(test)]
thread_local! {
    static RANK_CALLS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
fn record_rank_call() {
    RANK_CALLS.set(RANK_CALLS.get().saturating_add(1));
}

#[cfg(not(test))]
const fn record_rank_call() {}

#[cfg(test)]
pub(super) fn reset_rank_call_count() {
    RANK_CALLS.set(0);
}

#[cfg(test)]
pub(super) fn rank_call_count() -> usize {
    RANK_CALLS.get()
}

fn incompatible_sigils(candidate: &str, query: &str) -> bool {
    query
        .chars()
        .next()
        .filter(|character| is_sigil(*character))
        .is_some_and(|query_sigil| !candidate.starts_with(query_sigil))
}

fn query_starts_with_sigil(query: &str) -> bool {
    query.chars().next().is_some_and(is_sigil)
}

const fn is_sigil(character: char) -> bool {
    matches!(character, '$' | '/' | '@')
}

fn is_boundary(candidate: &[char], position: usize) -> bool {
    candidate
        .get(position)
        .is_some_and(|character| character.is_alphanumeric())
        && (position == 0
            || candidate
                .get(position - 1)
                .is_some_and(|character| !character.is_alphanumeric()))
}

#[cfg(test)]
mod tests {
    use super::{MatchClass, SearchField, secondary, token};

    #[test]
    fn exact_prefix_contiguous_boundary_and_sparse_matches_rank_in_order() {
        let query = "/ace";
        let mut ranked = [
            ("/a-very-long-circuitous-extended", "sparse"),
            ("/ace-tools", "prefix"),
            ("/trace", "contiguous"),
            ("/aos-communication-email", "boundary abbreviation"),
            ("/ace", "exact"),
        ]
        .into_iter()
        .map(|(candidate, label)| (token(candidate, query).expect("match"), label))
        .collect::<Vec<_>>();
        ranked.sort_by_key(|(rank, _)| *rank);
        assert_eq!(
            ranked
                .into_iter()
                .map(|(_, label)| label)
                .collect::<Vec<_>>(),
            [
                "exact",
                "prefix",
                "contiguous",
                "boundary abbreviation",
                "sparse"
            ]
        );
    }

    #[test]
    fn separators_case_and_canonical_unicode_are_explicit() {
        assert!(token("/AOS-Communication-Email", "/aos-ce").is_some());
        assert!(token("/café-tools", "/cafe\u{301}").is_some());
        assert!(token("/café-tools", "/cafe").is_none());
        assert!(token("/alpha_beta:gamma.delta", "/abgd").is_some());
        assert!(token("$skill", "/skill").is_none());
        assert!(token("@agent", "$agent").is_none());
    }

    #[test]
    fn contiguous_matching_uses_the_best_occurrence_not_only_the_first() {
        let later_boundary = token("/tracer-ace", "/ace").expect("later boundary");
        let no_boundary = token("/place", "/ace").expect("non-boundary");
        assert!(later_boundary < no_boundary);
    }

    #[test]
    fn contiguous_matching_checks_overlapping_boundary_occurrences() {
        let overlapping_boundary = token("/xa-a-a", "/a-a").expect("overlapping boundary");
        let non_boundary = token("/xa-a-z", "/a-a").expect("non-boundary");
        assert!(overlapping_boundary < non_boundary);
    }

    #[test]
    fn secondary_fields_never_outrank_token_matches_or_accept_sigils() {
        let primary = token("/media-image", "image").expect("token");
        let prose = secondary("image", "image").expect("description");
        assert!(primary < prose);
        assert!(secondary("send email", "/email").is_none());
        assert_eq!(primary.field, SearchField::Token);
        assert_eq!(prose.field, SearchField::Secondary);
    }

    #[test]
    fn empty_query_matches_without_disturbing_canonical_order() {
        let left = token("/zeta", "").expect("empty match");
        let right = token("$alpha", "").expect("empty match");
        assert_eq!(left, right);
        assert_eq!(left.class, MatchClass::Empty);
    }
}
