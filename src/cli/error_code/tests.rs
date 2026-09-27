//! Drift checks between emitted error codes and the public reference.

use std::collections::HashSet;

use super::{ErrorCode, reference_table};

const CLI_REFERENCE: &str = include_str!("../../../docs/reference/cli.md");

#[test]
fn reference_documents_every_emitted_error_code_exactly() {
    let table = reference_table();
    assert!(
        CLI_REFERENCE.contains(&table),
        "docs/reference/cli.md must contain this exact generated table:\n{table}"
    );
    let documented = CLI_REFERENCE
        .lines()
        .filter_map(|line| line.strip_prefix("| `"))
        .filter_map(|line| line.split_once('`').map(|(code, _)| code))
        .filter(|code| {
            code.chars()
                .all(|character| character == '_' || character.is_ascii_lowercase())
        })
        .collect::<HashSet<_>>();
    let emitted = ErrorCode::ALL
        .iter()
        .map(|code| code.as_str())
        .collect::<HashSet<_>>();
    assert_eq!(
        documented, emitted,
        "documented codes must equal emitted codes"
    );
}

#[test]
fn every_code_is_unique_and_uses_a_documented_exit_status() {
    let mut seen = HashSet::new();
    for code in ErrorCode::ALL {
        assert!(seen.insert(code.as_str()), "duplicate {}", code.as_str());
        assert!(
            matches!(code.exit(), 1..=8),
            "{} uses an undocumented exit status",
            code.as_str()
        );
        assert!(!code.details().is_empty());
    }
}
