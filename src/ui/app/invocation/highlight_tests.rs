use std::collections::BTreeSet;

use super::scan_content;

#[test]
fn exact_tokens_are_found_without_matching_partial_or_embedded_text() {
    let tokens = BTreeSet::from([
        "$review".to_owned(),
        "@agent-audit".to_owned(),
        "/plan".to_owned(),
    ]);
    let content = "/plan use $review, not $reviewer or mail@agent-audit";
    let ranges = scan_content(content, &tokens);
    let values = ranges
        .iter()
        .filter_map(|range| content.get(range.clone()))
        .collect::<Vec<_>>();
    assert_eq!(values, ["/plan", "$review"]);
}

#[test]
fn slash_starters_are_only_found_at_byte_zero() {
    let tokens = BTreeSet::from(["/goal".to_owned(), "/plan".to_owned()]);
    for content in [" /plan task", "text /goal task", "\n/plan task"] {
        assert!(scan_content(content, &tokens).is_empty(), "{content:?}");
    }
}

#[test]
fn fenced_code_and_shell_style_variables_remain_plain() {
    let tokens = BTreeSet::from(["$HOME".to_owned(), "$review".to_owned()]);
    let content = "$HOME\n```text\n$review\n```\n$review";
    let ranges = scan_content(content, &tokens);
    let values = ranges
        .iter()
        .filter_map(|range| content.get(range.clone()))
        .collect::<Vec<_>>();
    assert_eq!(values, ["$review"]);
}

#[test]
fn highlighting_is_bounded() {
    let tokens = BTreeSet::from(["$review".to_owned()]);
    let content = "$review ".repeat(300);
    assert_eq!(scan_content(&content, &tokens).len(), 128);
}
