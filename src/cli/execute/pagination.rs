//! Bounded list pages continued after a stable entry identity.

use crate::cli::error_code::ErrorCode;

use super::{super::args::PageArgs, CliError};

/// One bounded slice of an ordered projection.
pub(super) struct Page<T> {
    /// Entries in the requested slice, in projection order.
    pub(super) entries: Vec<T>,
    /// Number of entries in the complete projection.
    pub(super) total: usize,
    /// Continuation identity for the next page, when entries remain.
    pub(super) next_after: Option<String>,
}

/// Slice an ordered projection after an optional anchor identity.
///
/// `parse` validates the caller's anchor with the projection's typed identifier.
/// An anchor that is no longer present fails instead of silently restarting.
pub(super) fn paginate<T, K: PartialEq + ToString>(
    entries: Vec<T>,
    page: &PageArgs,
    key: impl Fn(&T) -> K,
    parse: impl FnOnce(&str) -> Result<K, CliError>,
) -> Result<Page<T>, CliError> {
    let total = entries.len();
    let start = match page.after.as_deref() {
        None => 0,
        Some(after) => {
            let anchor = parse(after)?;
            entries
                .iter()
                .position(|entry| key(entry) == anchor)
                .map(|index| index.saturating_add(1))
                .ok_or_else(|| {
                    CliError::new(
                        ErrorCode::CursorNotFound,
                        format!("continuation entry is no longer listed: {after}"),
                    )
                })?
        }
    };
    let limit = page
        .limit
        .and_then(|limit| usize::try_from(limit).ok())
        .unwrap_or(usize::MAX);
    let entries: Vec<T> = entries.into_iter().skip(start).take(limit).collect();
    let end = start.saturating_add(entries.len());
    let next_after = if end < total {
        entries.last().map(|entry| key(entry).to_string())
    } else {
        None
    };
    Ok(Page {
        entries,
        total,
        next_after,
    })
}

#[cfg(test)]
mod tests {
    use super::{PageArgs, paginate};
    use crate::cli::output::CliError;

    fn page(limit: Option<u32>, after: Option<&str>) -> PageArgs {
        PageArgs {
            limit,
            after: after.map(str::to_owned),
        }
    }

    fn parse(value: &str) -> Result<u32, CliError> {
        value
            .parse()
            .map_err(|_| CliError::identifier(value.to_owned()))
    }

    #[test]
    fn pages_continue_after_the_last_returned_identity_until_complete() {
        let entries = vec![1_u32, 2, 3, 4, 5];
        let first = paginate(entries.clone(), &page(Some(2), None), |entry| *entry, parse)
            .expect("first page");
        assert_eq!(first.entries, [1, 2]);
        assert_eq!(first.total, 5);
        assert_eq!(first.next_after.as_deref(), Some("2"));
        let second = paginate(
            entries.clone(),
            &page(Some(2), Some("2")),
            |entry| *entry,
            parse,
        )
        .expect("second page");
        assert_eq!(second.entries, [3, 4]);
        let last =
            paginate(entries, &page(Some(2), Some("4")), |entry| *entry, parse).expect("last page");
        assert_eq!(last.entries, [5]);
        assert_eq!(last.next_after, None);
    }

    #[test]
    fn unbounded_empty_and_oversized_limits_are_complete() {
        let complete =
            paginate(vec![1_u32, 2], &page(None, None), |entry| *entry, parse).expect("complete");
        assert_eq!(complete.entries, [1, 2]);
        assert_eq!(complete.next_after, None);
        let empty = paginate(
            Vec::<u32>::new(),
            &page(Some(1), None),
            |entry| *entry,
            parse,
        )
        .expect("empty");
        assert!(empty.entries.is_empty());
        assert_eq!(empty.total, 0);
        let large = paginate(
            vec![1_u32],
            &page(Some(u32::MAX), None),
            |entry| *entry,
            parse,
        )
        .expect("large");
        assert_eq!(large.entries, [1]);
        assert_eq!(large.next_after, None);
    }

    #[test]
    fn a_missing_or_malformed_anchor_fails_without_restarting() {
        assert!(paginate(vec![1_u32], &page(None, Some("9")), |entry| *entry, parse).is_err());
        assert!(paginate(vec![1_u32], &page(None, Some("x")), |entry| *entry, parse).is_err());
        let after_last = paginate(vec![1_u32], &page(None, Some("1")), |entry| *entry, parse)
            .expect("after last");
        assert!(after_last.entries.is_empty());
        assert_eq!(after_last.next_after, None);
    }
}
