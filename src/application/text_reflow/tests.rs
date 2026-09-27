use super::{TextReflowOutcome, large_paste_annotation, reflow};

#[test]
fn unsupported_control_preserves_large_paste_identity_as_no_change() {
    let content = format!("{}\u{7}", "line\n".repeat(12));
    let annotation =
        large_paste_annotation(&content, 0, content.len()).expect("large-paste annotation");

    let projection = reflow(&content, std::slice::from_ref(&annotation)).expect("valid reflow");

    assert!(matches!(projection.outcome, TextReflowOutcome::Unchanged));
    assert!(projection.changes.is_empty());
    assert_eq!(projection.annotation_origins, vec![(0, 0)]);
}
