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
fn partition_preserves_crossing_provenance_on_both_sides() {
    let (left, right) = partition_annotations("abécd", &[annotation(1, 6)], 4).expect("partition");
    assert_eq!(left, vec![annotation(1, 4)]);
    assert_eq!(right, vec![annotation(0, 2)]);
}

#[test]
fn extraction_closes_and_rejoins_one_crossing_annotation() {
    let (remaining, extracted) =
        extract_annotations("0123456789", &[annotation(1, 9)], 3..7).expect("extract");
    assert_eq!(remaining, vec![annotation(1, 5)]);
    assert_eq!(extracted, vec![annotation(0, 4)]);
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
fn multiple_attachment_identities_survive_partition_extract_and_merge() {
    let source = "firstsecondtail";
    let annotations = vec![attachment(0, 5, "one.txt"), attachment(5, 11, "two.png")];
    let (left, right) = partition_annotations(source, &annotations, 5).expect("partition");
    assert_eq!(left, vec![attachment(0, 5, "one.txt")]);
    assert_eq!(right, vec![attachment(0, 6, "two.png")]);

    let (remaining, extracted) =
        extract_annotations(source, &annotations, 3..8).expect("extract attachments");
    assert_eq!(
        remaining,
        vec![attachment(0, 3, "one.txt"), attachment(3, 6, "two.png")]
    );
    assert_eq!(
        extracted,
        vec![attachment(0, 2, "one.txt"), attachment(2, 5, "two.png")]
    );

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
