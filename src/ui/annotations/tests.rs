use crate::{
    domain::{ContentAnnotation, ContentAnnotationKind},
    ports::editor::{TextChange, TextChangeSet},
};

use super::rebase;

fn annotation(start: usize, end: usize) -> ContentAnnotation {
    ContentAnnotation {
        start,
        end,
        kind: ContentAnnotationKind::LargePaste {
            lines: 12,
            graphemes: end - start,
        },
    }
}

fn changes(
    before: &str,
    after: &str,
    ranges: &[(std::ops::Range<usize>, std::ops::Range<usize>)],
) -> TextChangeSet {
    let entries = ranges
        .iter()
        .map(|(old, new)| {
            TextChange::new(before, after, old.clone(), new.clone()).expect("valid change")
        })
        .collect();
    TextChangeSet::new(before, after, entries).expect("valid transaction")
}

#[test]
fn annotations_before_and_after_changes_are_preserved_and_shifted() {
    let before = "AA fold ZZ";
    let folded = annotation(3, 7);

    let after_prefix = "A fold ZZ";
    let prefix_changes = changes(before, after_prefix, &[(0..2, 0..1)]);
    assert_eq!(
        rebase(
            before,
            after_prefix,
            &prefix_changes,
            std::slice::from_ref(&folded),
            &[]
        ),
        [annotation(2, 6)]
    );

    let after_suffix = "AA fold Z";
    let suffix_changes = changes(before, after_suffix, &[(8..10, 8..9)]);
    assert_eq!(
        rebase(
            before,
            after_suffix,
            &suffix_changes,
            std::slice::from_ref(&folded),
            &[]
        ),
        [folded]
    );
}

#[test]
fn enclosing_enclosed_and_intersecting_destructive_edits_dissolve_annotations() {
    let before = "AA fold ZZ";
    let folded = annotation(3, 7);
    for (after, old, new) in [
        ("AA fld ZZ", 4..5, 4..4),
        ("AA-ZZ", 2..8, 2..3),
        ("A+old ZZ", 1..4, 1..2),
        ("AA fol+Z", 6..9, 6..7),
    ] {
        let transaction = changes(before, after, &[(old, new)]);
        assert!(
            rebase(
                before,
                after,
                &transaction,
                std::slice::from_ref(&folded),
                &[],
            )
            .is_empty()
        );
    }
}

#[test]
fn insertions_inside_dissolve_but_insertions_at_boundaries_stay_outside() {
    let before = "AA fold ZZ";
    let folded = annotation(3, 7);

    let inside = "AA fo+ld ZZ";
    let inside_changes = changes(before, inside, &[(5..5, 5..6)]);
    assert!(
        rebase(
            before,
            inside,
            &inside_changes,
            std::slice::from_ref(&folded),
            &[],
        )
        .is_empty()
    );

    let at_start = "AA +fold ZZ";
    let start_changes = changes(before, at_start, &[(3..3, 3..4)]);
    assert_eq!(
        rebase(
            before,
            at_start,
            &start_changes,
            std::slice::from_ref(&folded),
            &[],
        ),
        [annotation(4, 8)]
    );

    let at_end = "AA fold+ ZZ";
    let end_changes = changes(before, at_end, &[(7..7, 7..8)]);
    assert_eq!(
        rebase(before, at_end, &end_changes, &[folded], &[]),
        [annotation(3, 7)]
    );
}

#[test]
fn destructive_edits_exactly_touching_boundaries_preserve_annotations() {
    let before = "AA fold ZZ";
    let folded = annotation(3, 7);

    let remove_before = "AAfold ZZ";
    let before_changes = changes(before, remove_before, &[(2..3, 2..2)]);
    assert_eq!(
        rebase(
            before,
            remove_before,
            &before_changes,
            std::slice::from_ref(&folded),
            &[],
        ),
        [annotation(2, 6)]
    );

    let remove_after = "AA foldZZ";
    let after_changes = changes(before, remove_after, &[(7..8, 7..7)]);
    assert_eq!(
        rebase(before, remove_after, &after_changes, &[folded], &[]),
        [annotation(3, 7)]
    );
}

#[test]
fn two_disjoint_edits_preserve_an_annotation_in_the_untouched_middle() {
    let before = "left MIDDLE right";
    let after = "L MIDDLE R";
    let transaction = changes(before, after, &[(0..4, 0..1), (12..17, 9..10)]);

    assert_eq!(
        rebase(before, after, &transaction, &[annotation(5, 11)], &[]),
        [annotation(2, 8)]
    );
}

#[test]
fn inserted_annotations_are_anchored_to_the_single_resulting_change_range() {
    let before = "AA ZZ";
    let after = "AA 路径ZZ";
    let inserted_text = "路径";
    let transaction = changes(before, after, &[(3..3, 3..3 + inserted_text.len())]);

    assert_eq!(
        rebase(
            before,
            after,
            &transaction,
            &[],
            &[annotation(0, inserted_text.len())],
        ),
        [annotation(3, 3 + inserted_text.len())]
    );
}

#[test]
fn invocation_reference_projects_without_placeholder_brackets() {
    let canonical = "Herdr collaborator: coaching-philipp (claude) at workspace Consulting (w4), tab coaching-philipp (w4:t2), pane w4:p2";
    let display = "@coaching-philipp · claude";
    let projected = super::project(
        canonical,
        &[ContentAnnotation {
            start: 0,
            end: canonical.len(),
            kind: ContentAnnotationKind::InvocationReference {
                display_name: display.to_owned(),
            },
        }],
        &[],
    );

    assert_eq!(projected.content, display);
    assert_eq!(projected.folds.len(), 1);
    assert_eq!(projected.folds[0].canonical_end, canonical.len());
}
