use proqi::ports::editor::{
    OffsetAffinity, TextChange, TextChangeError, TextChangeSet, TextCoordinateSpace,
};

#[test]
fn validates_disjoint_coordinates_and_maps_offsets() {
    let before = "aa middle zz";
    let after = "A middle Z";
    let changes = TextChangeSet::new(
        before,
        after,
        vec![
            TextChange::new(before, after, 0..2, 0..1).expect("first change"),
            TextChange::new(before, after, 10..12, 9..10).expect("second change"),
        ],
    )
    .expect("valid disjoint transaction");

    assert_eq!(changes.len(), 2);
    assert_eq!(map(&changes, before, 3, OffsetAffinity::Before), 2);
    assert_eq!(map(&changes, before, 9, OffsetAffinity::After), 8);
    assert_eq!(map(&changes, before, 10, OffsetAffinity::Before), 9);
    assert_eq!(map(&changes, before, 10, OffsetAffinity::After), 10);
    assert_eq!(changes.inverse().inverse(), changes);
}

#[test]
fn rejects_empty_mismatch_order_overlap_and_ineffective_entries() {
    assert!(TextChangeSet::new("same", "same", Vec::new()).is_ok());
    assert_eq!(
        TextChangeSet::new("old", "new", Vec::new()),
        Err(TextChangeError::UnrepresentedContent)
    );

    let before = "aa middle zz";
    let after = "A middle Z";
    let first = TextChange::new(before, after, 0..2, 0..1).expect("first");
    let second = TextChange::new(before, after, 10..12, 9..10).expect("second");
    assert_eq!(
        TextChangeSet::new(before, after, vec![second, first]),
        Err(TextChangeError::UnorderedOrOverlapping { index: 1 })
    );

    let overlap_before = "abcd";
    let overlap_after = "XYd";
    let left = TextChange::new(overlap_before, overlap_after, 0..2, 0..1).expect("left");
    let right = TextChange::new(overlap_before, overlap_after, 1..3, 1..2).expect("right");
    assert_eq!(
        TextChangeSet::new(overlap_before, overlap_after, vec![left, right]),
        Err(TextChangeError::UnorderedOrOverlapping { index: 1 })
    );

    let unchanged = TextChange::new("same", "same", 0..4, 0..4).expect("range");
    assert_eq!(
        TextChangeSet::new("same", "same", vec![unchanged]),
        Err(TextChangeError::UnchangedEntry { index: 0 })
    );
}

#[test]
fn rejects_invalid_utf8_boundaries_and_mapping_offsets() {
    assert_eq!(
        TextChange::new("é", "x", 1..2, 0..1),
        Err(TextChangeError::InvalidUtf8Boundary {
            space: TextCoordinateSpace::Before,
            offset: 1,
        })
    );
    let before = "éx";
    let after = "é!x";
    let change = TextChange::new(before, after, 2..2, 2..3).expect("insertion");
    let changes = TextChangeSet::new(before, after, vec![change]).expect("transaction");
    assert_eq!(
        changes.map_old_offset(before, 1, OffsetAffinity::Before),
        Err(TextChangeError::InvalidUtf8Boundary {
            space: TextCoordinateSpace::Before,
            offset: 1,
        })
    );
    assert_eq!(
        TextChange::new("x", "é", 0..1, 1..2),
        Err(TextChangeError::InvalidUtf8Boundary {
            space: TextCoordinateSpace::After,
            offset: 1,
        })
    );
}

#[test]
fn offset_mapping_has_explicit_insertion_and_deletion_affinity() {
    let insertion_before = "ab";
    let insertion_after = "a++b";
    let insertion =
        TextChange::new(insertion_before, insertion_after, 1..1, 1..3).expect("insertion range");
    let insertion = TextChangeSet::new(insertion_before, insertion_after, vec![insertion])
        .expect("insertion transaction");
    assert_eq!(
        map(&insertion, insertion_before, 1, OffsetAffinity::Before),
        1
    );
    assert_eq!(
        map(&insertion, insertion_before, 1, OffsetAffinity::After),
        3
    );
    assert_eq!(
        map(&insertion, insertion_before, 2, OffsetAffinity::Before),
        4
    );

    let deletion_before = "abcd";
    let deletion_after = "ad";
    let deletion =
        TextChange::new(deletion_before, deletion_after, 1..3, 1..1).expect("deletion range");
    let deletion = TextChangeSet::new(deletion_before, deletion_after, vec![deletion])
        .expect("deletion transaction");
    for affinity in [OffsetAffinity::Before, OffsetAffinity::After] {
        assert_eq!(map(&deletion, deletion_before, 2, affinity), 1);
    }
}

fn map(changes: &TextChangeSet, before: &str, offset: usize, affinity: OffsetAffinity) -> usize {
    changes
        .map_old_offset(before, offset, affinity)
        .expect("valid mapped offset")
}
