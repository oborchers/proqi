use super::*;
use crate::domain::ContentAnnotationKind;

fn annotation(start: usize, end: usize) -> ContentAnnotation {
    ContentAnnotation {
        start,
        end,
        kind: ContentAnnotationKind::LargePaste {
            lines: 3,
            graphemes: 8,
        },
    }
}

fn attachment(start: usize, end: usize, display_name: &str) -> ContentAnnotation {
    ContentAnnotation {
        start,
        end,
        kind: ContentAnnotationKind::Attachment {
            image: std::path::Path::new(display_name)
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("png")),
            display_name: display_name.to_owned(),
        },
    }
}

#[test]
fn partition_dissolves_a_crossing_semantic_annotation() {
    let (left, right) = partition_annotations("abécd", &[annotation(1, 6)], 4).expect("partition");
    assert!(left.is_empty());
    assert!(right.is_empty());
}

#[test]
fn extraction_dissolves_one_crossing_semantic_annotation() {
    let (remaining, extracted) =
        extract_annotations("0123456789", &[annotation(1, 9)], 3..7).expect("extract");
    assert!(remaining.is_empty());
    assert!(extracted.is_empty());
}

#[test]
fn merge_shifts_every_annotation_by_exact_separator_length() {
    let first = vec![annotation(0, 2)];
    let second = vec![annotation(1, 3)];
    let merged = merge_annotations(
        [("ab", first.as_slice()), ("xyz", second.as_slice())],
        "\r\n",
    )
    .expect("merge");
    assert_eq!(merged, vec![annotation(0, 2), annotation(5, 7)]);
}

#[test]
fn range_operations_reject_split_utf8_and_empty_extraction() {
    assert_eq!(
        partition_annotations("é", &[], 1),
        Err(DomainError::InvalidContentRange)
    );
    assert_eq!(
        extract_annotations("text", &[], 2..2),
        Err(DomainError::EmptyContentRange)
    );
}

#[test]
fn adjacent_equal_annotations_remain_distinct_fold_identities() {
    let annotations = vec![annotation(0, 2), annotation(2, 4)];
    let (left, right) = partition_annotations("abcd", &annotations, 2).expect("partition");
    assert_eq!(left, vec![annotation(0, 2)]);
    assert_eq!(right, vec![annotation(0, 2)]);

    let (remaining, extracted) =
        extract_annotations("abcdef", &annotations, 4..6).expect("extract adjacent");
    assert_eq!(remaining, annotations);
    assert!(extracted.is_empty());
}

#[test]
fn attachments_survive_only_when_their_complete_ranges_survive() {
    let source = "firstsecondtail";
    let annotations = vec![attachment(0, 5, "one.txt"), attachment(5, 11, "two.png")];
    let (left, right) = partition_annotations(source, &annotations, 5).expect("partition");
    assert_eq!(left, vec![attachment(0, 5, "one.txt")]);
    assert_eq!(right, vec![attachment(0, 6, "two.png")]);

    let (remaining, extracted) =
        extract_annotations(source, &annotations, 3..8).expect("extract attachments");
    assert!(remaining.is_empty());
    assert!(extracted.is_empty());

    let merged = merge_annotations(
        [("first", left.as_slice()), ("second", right.as_slice())],
        "\n\n",
    )
    .expect("merge attachments");
    assert_eq!(
        merged,
        vec![attachment(0, 5, "one.txt"), attachment(7, 13, "two.png")]
    );
}

#[test]
fn extraction_moves_intact_attachment_and_shortcut_ranges_exactly() {
    let shortcut = ContentAnnotation::shortcut(5, 10);
    let annotations = vec![attachment(0, 4, "one.txt"), shortcut.clone()];
    let (remaining, extracted) =
        extract_annotations("path Enter tail", &annotations, 5..10).expect("extract intact");
    assert_eq!(remaining, vec![attachment(0, 4, "one.txt")]);
    assert_eq!(extracted, vec![ContentAnnotation::shortcut(0, 5)]);

    let (left, right) =
        partition_annotations("path Enter tail", &annotations, 7).expect("cross shortcut");
    assert_eq!(left, vec![attachment(0, 4, "one.txt")]);
    assert!(right.is_empty());
}
