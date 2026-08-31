use super::{HighlightTokens, scan_content};

fn tokens(anywhere: &[&str], document_start: &[&str]) -> HighlightTokens {
    HighlightTokens {
        anywhere: anywhere.iter().map(|token| (*token).to_owned()).collect(),
        document_start: document_start
            .iter()
            .map(|token| (*token).to_owned())
            .collect(),
    }
}

#[test]
fn exact_tokens_are_found_without_matching_partial_or_embedded_text() {
    let tokens = tokens(&["$review", "@agent-audit"], &["/plan"]);
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
    let tokens = tokens(&[], &["/goal", "/plan"]);
    for content in [" /plan task", "text /goal task", "\n/plan task"] {
        assert!(scan_content(content, &tokens).is_empty(), "{content:?}");
    }
}

#[test]
fn discovered_slash_forms_are_found_inline_and_on_later_lines() {
    let tokens = tokens(&["/implement-in-worktree"], &[]);
    let content = "Use /implement-in-worktree here\n/implement-in-worktree again";
    let ranges = scan_content(content, &tokens);
    let values = ranges
        .iter()
        .filter_map(|range| content.get(range.clone()))
        .collect::<Vec<_>>();
    assert_eq!(values, ["/implement-in-worktree", "/implement-in-worktree"]);
}

#[test]
fn paths_urls_partial_and_non_boundary_slash_forms_remain_plain() {
    let tokens = tokens(&["/implement-in-worktree"], &[]);
    for content in [
        "/tmp/implement-in-worktree",
        "https://example.test/implement-in-worktree",
        "x/implement-in-worktree",
        "/implement-in-worktree-extra",
        "```text\n/implement-in-worktree\n```",
    ] {
        assert!(scan_content(content, &tokens).is_empty(), "{content:?}");
    }
}

#[test]
fn fenced_code_and_shell_style_variables_remain_plain() {
    let tokens = tokens(&["$HOME", "$review"], &[]);
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
    let tokens = tokens(&["$review"], &[]);
    let content = "$review ".repeat(300);
    assert_eq!(scan_content(&content, &tokens).len(), 128);
}
