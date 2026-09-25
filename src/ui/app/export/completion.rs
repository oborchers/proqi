//! Per-directory prefix completion for the export destination field.
//!
//! Listing happens on the external lane. This module only splits the typed text,
//! resolves the directory to list, filters a returned listing, and chooses the
//! completed text, so it stays deterministic and filesystem-free.

use std::path::{Path, PathBuf};

use crate::ports::export::DirectoryListing;

/// Most candidates retained for display and cycling.
pub(super) const MAX_CANDIDATES: usize = 64;

/// The directory to list and how its entries complete the typed text.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct CompletionRequest {
    /// Absolute directory whose entries complete the final path component.
    pub(super) directory: PathBuf,
    /// Typed text up to and including the last `/`, kept verbatim.
    pub(super) prefix: String,
    /// Typed final component being completed.
    pub(super) partial: String,
}

/// One completion choice.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct Candidate {
    /// Entry name as displayed, with a trailing `/` for folders.
    pub(super) label: String,
    /// Complete field text after choosing this candidate.
    pub(super) text: String,
}

/// Result of applying one listing to the typed text.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum CompletionOutcome {
    /// Nothing in the folder starts with the typed name.
    NoMatch,
    /// The field text becomes this value. Remaining choices, if any, are offered.
    Completed {
        /// New field text.
        text: String,
        /// Choices still sharing the completed prefix, empty when unique.
        candidates: Vec<Candidate>,
    },
    /// Several choices share no longer prefix; the caller cycles through them.
    Ambiguous(Vec<Candidate>),
}

/// Split the typed text into the folder to list and the partial final name.
pub(super) fn completion_request(
    text: &str,
    base: &Path,
    home: Option<&Path>,
) -> Option<CompletionRequest> {
    let split = text.rfind('/').map_or(0, |index| index + 1);
    let (prefix, partial) = text.split_at(split);
    let directory = if prefix.is_empty() {
        base.to_path_buf()
    } else if let Some(rest) = prefix.strip_prefix("~/") {
        home.filter(|home| home.is_absolute())?.join(rest)
    } else if prefix.starts_with('/') {
        PathBuf::from(prefix)
    } else {
        base.join(prefix)
    };
    Some(CompletionRequest {
        directory,
        prefix: prefix.to_owned(),
        partial: partial.to_owned(),
    })
}

/// Choose the completed text for one listing.
pub(super) fn complete(
    request: &CompletionRequest,
    listing: &DirectoryListing,
) -> CompletionOutcome {
    let show_hidden = request.partial.starts_with('.');
    let candidates = listing
        .entries
        .iter()
        .filter(|entry| entry.name.starts_with(&request.partial))
        .filter(|entry| show_hidden || !entry.name.starts_with('.'))
        .take(MAX_CANDIDATES)
        .map(|entry| {
            let label = if entry.directory {
                format!("{}/", entry.name)
            } else {
                entry.name.clone()
            };
            Candidate {
                text: format!("{}{label}", request.prefix),
                label,
            }
        })
        .collect::<Vec<_>>();
    match candidates.as_slice() {
        [] => CompletionOutcome::NoMatch,
        [only] => CompletionOutcome::Completed {
            text: only.text.clone(),
            candidates: Vec::new(),
        },
        _ => {
            let shared = common_prefix(candidates.iter().map(|candidate| candidate.label.as_str()));
            if shared.len() > request.partial.len() {
                CompletionOutcome::Completed {
                    text: format!("{}{shared}", request.prefix),
                    candidates,
                }
            } else {
                CompletionOutcome::Ambiguous(candidates)
            }
        }
    }
}

/// Longest shared prefix on character boundaries.
fn common_prefix<'a>(mut values: impl Iterator<Item = &'a str>) -> String {
    let Some(first) = values.next() else {
        return String::new();
    };
    let mut end = first.len();
    for value in values {
        end = first
            .char_indices()
            .zip(value.chars())
            .take_while(|((_, left), right)| left == right)
            .last()
            .map_or(0, |((index, character), _)| index + character.len_utf8())
            .min(end);
    }
    first[..end].to_owned()
}

#[cfg(test)]
#[path = "completion/tests.rs"]
mod tests;
