//! Plain-text thought export values shared by the terminal and scriptable interfaces.
//!
//! Everything here is pure: default names, file-name sanitizing, and path resolution
//! never touch the filesystem. Writing, listing, and durability belong to adapters.

use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::{ThoughtName, Timestamp};

/// Extension appended to every default export name.
pub const EXPORT_EXTENSION: &str = "txt";

/// Maximum UTF-8 byte length of a sanitized default file stem.
pub const EXPORT_STEM_MAX_BYTES: usize = 200;

const FALLBACK_STEM: &str = "thoughts";
const MILLIS_PER_SECOND: i64 = 1_000;
const SECONDS_PER_DAY: i64 = 86_400;

/// What happens to the exported thoughts once the file is durable.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportDisposition {
    /// Write the file and leave the board unchanged.
    Keep,
    /// Write the file, then recoverably delete the exported thoughts.
    Remove,
    /// Write the file, then replace the exported thoughts with one file reference.
    ReplaceWithReference,
}

impl ExportDisposition {
    /// Whether the board changes after a durable write.
    #[must_use]
    pub const fn changes_board(self) -> bool {
        !matches!(self, Self::Keep)
    }

    /// Stable machine spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Keep => "keep",
            Self::Remove => "remove",
            Self::ReplaceWithReference => "replace_with_reference",
        }
    }
}

/// Why a typed destination cannot name an export file.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum ExportPathError {
    /// The destination is empty or only whitespace.
    #[error("enter a file path")]
    Empty,
    /// The destination starts with `~` but no home directory is known.
    #[error("the home directory is unknown")]
    HomeUnavailable,
    /// The destination names a directory rather than a file.
    #[error("the path must end with a file name")]
    MissingFileName,
    /// The resolution base is not absolute.
    #[error("the base directory is not absolute")]
    RelativeBase,
}

/// Resolve a typed destination against a base directory.
///
/// `~` and `~/…` resolve from `home`. Other relative paths resolve from `base`.
/// `.` and `..` are resolved lexically, so the written path, the reference
/// thought, and the receipt all name the same clean absolute path. As in the
/// shell's logical view, `..` after a symbolic link returns to the folder that
/// named the link.
///
/// # Errors
///
/// Returns a typed error for an empty value, an unknown home directory, a relative
/// base, or a destination without a final file name.
pub fn resolve_export_path(
    input: &str,
    base: &Path,
    home: Option<&Path>,
) -> Result<PathBuf, ExportPathError> {
    if input.trim().is_empty() {
        return Err(ExportPathError::Empty);
    }
    if !base.is_absolute() {
        return Err(ExportPathError::RelativeBase);
    }
    if input.ends_with('/') {
        return Err(ExportPathError::MissingFileName);
    }
    let joined = if input == "~" {
        return Err(ExportPathError::MissingFileName);
    } else if let Some(rest) = input.strip_prefix("~/") {
        let home = home.ok_or(ExportPathError::HomeUnavailable)?;
        if !home.is_absolute() {
            return Err(ExportPathError::HomeUnavailable);
        }
        home.join(rest)
    } else {
        base.join(input)
    };
    // The typed value must itself end in a file name, not in `.` or `..`.
    // `Path::components` drops a trailing `.`, so that case is checked on the text.
    if input == "."
        || input.ends_with("/.")
        || !matches!(
            Path::new(input).components().next_back(),
            Some(Component::Normal(_))
        )
    {
        return Err(ExportPathError::MissingFileName);
    }
    let mut normalized = PathBuf::new();
    for component in joined.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                // The root's parent is the root.
                let _popped = normalized.pop();
            }
            component => normalized.push(component),
        }
    }
    match normalized.components().next_back() {
        Some(Component::Normal(_)) => Ok(normalized),
        _ => Err(ExportPathError::MissingFileName),
    }
}

/// Default export file name for a selection.
///
/// One selected thought with a name uses that name. Everything else uses the
/// session label plus a UTC timestamp. The result is sanitized and ends in `.txt`.
#[must_use]
pub fn default_export_file_name(
    single_name: Option<&ThoughtName>,
    session_label: &str,
    at: Timestamp,
) -> String {
    let stem = single_name.map_or_else(
        || {
            format!(
                "{}-{}",
                sanitize_file_stem(session_label),
                utc_file_timestamp(at)
            )
        },
        |name| sanitize_file_stem(name.as_str()),
    );
    format!("{stem}.{EXPORT_EXTENSION}")
}

/// Replace characters that are unsafe or awkward in file names across platforms.
///
/// Path separators, controls, and `:*?"<>|` become `-`, whitespace runs become
/// one space, leading dots and surrounding spaces are removed, and the stem is
/// capped at [`EXPORT_STEM_MAX_BYTES`] on a character boundary.
#[must_use]
pub fn sanitize_file_stem(raw: &str) -> String {
    let mut stem = String::with_capacity(raw.len());
    let mut previous_space = false;
    for character in raw.chars() {
        if character.is_whitespace() {
            if !previous_space {
                stem.push(' ');
            }
            previous_space = true;
            continue;
        }
        previous_space = false;
        if character.is_control()
            || matches!(
                character,
                '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'
            )
        {
            stem.push('-');
        } else {
            stem.push(character);
        }
    }
    let trimmed = stem.trim().trim_start_matches('.').trim_start();
    let mut end = trimmed.len().min(EXPORT_STEM_MAX_BYTES);
    while !trimmed.is_char_boundary(end) {
        end -= 1;
    }
    let bounded = trimmed[..end].trim_end();
    if bounded.is_empty() {
        FALLBACK_STEM.to_owned()
    } else {
        bounded.to_owned()
    }
}

/// Format a timestamp as `YYYY-MM-DD-HHMMSS` in UTC.
#[must_use]
pub fn utc_file_timestamp(at: Timestamp) -> String {
    let seconds = at.as_millis().div_euclid(MILLIS_PER_SECOND);
    let days = seconds.div_euclid(SECONDS_PER_DAY);
    let second_of_day = seconds.rem_euclid(SECONDS_PER_DAY);
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}-{:02}{:02}{:02}",
        second_of_day / 3_600,
        second_of_day % 3_600 / 60,
        second_of_day % 60
    )
}

/// Convert days since 1970-01-01 to a proleptic Gregorian date (Howard Hinnant's algorithm).
const fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    let year = year_of_era + era * 400 + if month <= 2 { 1 } else { 0 };
    (year, month, day)
}

#[cfg(test)]
#[path = "export/tests.rs"]
mod tests;
