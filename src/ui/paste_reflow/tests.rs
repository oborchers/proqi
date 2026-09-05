//! True-positive, false-positive, and bounded-work fixtures for paste reflow.

use std::fmt::Write as _;

use super::reflow_text;

fn reflow(input: &str) -> String {
    reflow_text(input, &[]).expect("fixture reflows").content
}

#[test]
fn terminal_wrapped_prose_becomes_paragraphs() {
    let input = "The entire reflow-created thought disappears in one undo step. Press Command+Shift+V to\nredo it, and the same saved one-paragraph thought returns. This uses the existing\npersistent revision machinery rather than maintaining separate paste history.\n\n\nThe original system clipboard has already been restored.";
    assert_eq!(
        reflow(input),
        "The entire reflow-created thought disappears in one undo step. Press Command+Shift+V to redo it, and the same saved one-paragraph thought returns. This uses the existing persistent revision machinery rather than maintaining separate paste history.\n\nThe original system clipboard has already been restored."
    );
}

#[test]
fn prose_cleanup_is_width_independent_and_idempotent() {
    for input in [
        "  Narrow\t source\n  wrapping  ",
        "  Narrow\t source wrapping  ",
        "Wide source text that only wraps after several additional words\nand continues here.",
        "First hard break  \nsecond hard break\\\nthird line",
        "Gedanken mit Umlauten äöü\nund 日本語 sowie e\u{301} und 👩🏽‍💻.",
    ] {
        let once = reflow(input);
        assert_eq!(reflow(&once), once, "input {input:?}");
        assert!(!once.contains('\t'));
        assert!(!once.contains("  "), "output {once:?}");
    }
}

#[test]
fn crlf_paragraph_family_is_retained() {
    assert_eq!(
        reflow("  first\r\nline\r\n\r\n\r\n second\r\nparagraph  \r\n"),
        "first line\r\n\r\nsecond paragraph"
    );
}

#[test]
fn list_markers_and_nesting_survive_aligned_continuations() {
    let input = "- First  item\n  wraps here\n- [x] Second\titem\n      stays aligned\n  - Nested  item\n    wraps too\n1. Ordered  item\n   continuation\n2) Neighbor";
    assert_eq!(
        reflow(input),
        "- First item wraps here\n- [x] Second item stays aligned\n  - Nested item wraps too\n1. Ordered item continuation\n2) Neighbor"
    );
}

#[test]
fn unaligned_list_continuations_keep_their_line_boundary() {
    assert_eq!(
        reflow("- item\n continuation at the wrong column\n- neighbor"),
        "- item\ncontinuation at the wrong column\n- neighbor"
    );
}

#[test]
fn prose_next_to_lists_reflows_without_crossing_list_boundaries() {
    assert_eq!(
        reflow("Introductory  prose\nwraps here\n- item\nTrailing  prose\nwraps too"),
        "Introductory prose wraps here\n- item\nTrailing prose wraps too"
    );
}

#[test]
fn a_second_list_paragraph_keeps_its_owner_indent_and_reflows() {
    assert_eq!(
        reflow("- item\n\n  second  paragraph\n  wraps here\n\noutside\nwraps too"),
        "- item\n\n  second paragraph wraps here\n\noutside wraps too"
    );
}

#[test]
fn structural_and_meaning_sensitive_blocks_are_exact() {
    for input in [
        "```rust\nfn main() {\n    println!(\"x\");\n}\n```",
        "    indented  code\n    keeps   spaces",
        "| Name | Value |\n| --- | --- |\n| a | b |",
        "Name          Value\nAlice         42\nBob           7",
        "Name\tValue\nAlice\t42",
        "Owner       Value\n👩🏽‍💻          one\n界界        two",
        "Status  queued\nStatus  done",
        "> quoted line\n> another line",
        "# Heading\nintentional body line",
        "#\tHeading\nintentional body line",
        "######\nintentional body line",
        "Heading\n=======\nintentional body line",
        "Heading\n=\nintentional body line",
        "Heading\n--\nintentional body line",
        "Read https://example.com/a\nbefore continuing",
        "Open /usr/local/bin/proqi\nthen continue",
        "Open C:\\Program Files\\Proqi\nthen continue",
        "src/ui/app.rs\ntests/ui_board.rs",
        "Cargo.toml\nREADME.md",
        ".gitignore\n.rustfmt.toml",
        "---\nsection boundary",
        "_ _ _\nsection boundary",
        "alpha\u{7}beta\nwrapped",
        "alpha\r\nbeta\rlone carriage return",
        "alpha\u{2028}beta",
    ] {
        assert_eq!(reflow(input), input, "input {input:?}");
    }
}

#[test]
fn deeply_indented_list_like_code_is_exact_without_list_context() {
    let input = "    - shell command\n      --flag  value";
    assert_eq!(reflow(input), input);
}

#[test]
fn blank_runs_inside_indented_code_are_exact() {
    let input = "    first  line\n\n\n    second   line";
    assert_eq!(reflow(input), input);
}

#[test]
fn only_backslashes_followed_by_a_line_delimiter_are_hard_breaks() {
    assert_eq!(reflow("joined\\\nline\\"), "joined line\\");
    assert_eq!(reflow("literal\\"), "literal\\");
}

#[test]
fn many_aligned_fields_remain_exact_without_per_column_rescans() {
    let first = aligned_row("field");
    let second = aligned_row("value");
    let input = format!("{first}\n{second}");
    assert_eq!(reflow(&input), input);
}

fn aligned_row(prefix: &str) -> String {
    let mut output = String::new();
    for index in 0..4_096 {
        write!(&mut output, "{prefix}{index}  ").expect("string formatting succeeds");
    }
    output
}

#[test]
fn a_protected_annotation_keeps_its_complete_block_exact() {
    let input = "prefix  @agent\nwrapped  text\n\nordinary\nparagraph";
    let start = input.find("@agent").expect("annotation start");
    let protected = start..start + "@agent".len();
    assert_eq!(
        reflow_text(input, std::slice::from_ref(&protected))
            .expect("reflow")
            .content,
        "prefix  @agent\nwrapped  text\n\nordinary paragraph"
    );
}

#[test]
fn whitespace_only_input_becomes_empty() {
    for input in ["", "   ", "\n\n", " \r\n\t\r\n"] {
        assert_eq!(reflow(input), "", "input {input:?}");
    }
}

#[test]
fn multi_megabyte_reflow_uses_a_bounded_number_of_changes() {
    let line = "Terminal copied prose with repeated    spaces and enough content to wrap.\n";
    let input = line.repeat(32_768);
    let result = reflow_text(&input, &[]).expect("large reflow");
    assert!(result.content.len() < input.len());
    assert!(result.changes.len() <= 2);
    assert!(!result.content.contains('\n'));
    assert_eq!(reflow(&result.content), result.content);
}
