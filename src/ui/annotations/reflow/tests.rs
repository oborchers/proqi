//! Annotation and large-fold contracts across explicit paste reflow.

use std::fmt::Write as _;

use crate::domain::{ContentAnnotation, ContentAnnotationKind};
use unicode_segmentation::UnicodeSegmentation as _;

use super::{PastePayload, PasteReflow};

fn changed(payload: &PastePayload) -> PastePayload {
    match payload.reflow().expect("valid payload") {
        PasteReflow::Changed(payload) => payload,
        PasteReflow::Unchanged => panic!("expected changed payload"),
        PasteReflow::Empty => panic!("expected nonempty payload"),
    }
}

#[test]
fn reflow_drops_a_large_fold_that_falls_below_both_thresholds() {
    let eleven_lines = (0..11).map(|_| "line").collect::<Vec<_>>().join("\n");
    assert!(PastePayload::text(eleven_lines).annotations.is_empty());
    let content = (0..12)
        .map(|index| format!("short line {index}"))
        .collect::<Vec<_>>()
        .join("\n");
    let payload = PastePayload::text(content);
    assert!(matches!(
        payload.annotations.as_slice(),
        [ContentAnnotation {
            kind: ContentAnnotationKind::LargePaste { lines: 12, .. },
            ..
        }]
    ));
    let result = changed(&payload);
    assert_eq!(result.content.lines().count(), 1);
    assert!(result.annotations.is_empty());
}

#[test]
fn verified_attachment_paths_survive_changes_in_a_neighboring_prose_block() {
    let path = "/tmp/verified image.png";
    let content = format!("first\nparagraph\n\n{path}");
    let start = content.find(path).expect("path");
    let payload = PastePayload::attachments(
        content,
        vec![(
            start..start + path.len(),
            true,
            "verified image.png".to_owned(),
        )],
    )
    .with_verified_attachments();
    let result = changed(&payload);
    assert_eq!(result.verified_paths, vec![path]);
    assert_eq!(result.annotations.len(), 1);
    assert_eq!(
        &result.content[result.annotations[0].start..result.annotations[0].end],
        path
    );
}

#[test]
fn reflow_retains_and_recomputes_a_large_fold_at_the_grapheme_boundary() {
    for (graphemes, retained) in [(1_199, false), (1_200, true)] {
        let content = format!("{}\n tail", "a".repeat(graphemes - 5));
        let payload = PastePayload {
            content: content.clone(),
            annotations: vec![ContentAnnotation {
                start: 0,
                end: content.len(),
                kind: ContentAnnotationKind::LargePaste {
                    lines: 99,
                    graphemes: 99,
                },
            }],
            verified_paths: Vec::new(),
            preserve_owned_annotations: false,
        };
        let result = changed(&payload);
        assert_eq!(result.annotations.len(), usize::from(retained));
        if retained {
            assert!(matches!(
                result.annotations[0].kind,
                ContentAnnotationKind::LargePaste {
                    lines: 1,
                    graphemes: 1_200
                }
            ));
            assert_eq!(result.annotations[0].end, result.content.len());
        }
    }
}

#[test]
fn protected_semantic_annotations_rebase_without_changing_their_bytes() {
    let content = "first  line\nwraps here\n\n@agent  stays\nexact".to_owned();
    let start = content.find("@agent").expect("reference");
    let payload = PastePayload::preserved_clipboard(
        content,
        vec![ContentAnnotation {
            start,
            end: start + 6,
            kind: ContentAnnotationKind::InvocationReference {
                display_name: "@agent · codex".to_owned(),
            },
        }],
    )
    .expect("valid annotation");
    let result = changed(&payload);
    assert_eq!(
        result.content,
        "first line wraps here\n\n@agent  stays\nexact"
    );
    assert_eq!(
        &result.content[result.annotations[0].start..result.annotations[0].end],
        "@agent"
    );
    assert!(result.preserve_owned_annotations);
}

#[test]
fn protected_shortcut_emphasis_survives_reflow_and_invalid_metadata_fails_closed() {
    let content = "first\nparagraph\n\nCmd+Shift+V  remains".to_owned();
    let start = content.find("Cmd").expect("shortcut");
    let valid = PastePayload {
        content: content.clone(),
        annotations: vec![ContentAnnotation::shortcut(
            start,
            start + "Cmd+Shift+V".len(),
        )],
        verified_paths: Vec::new(),
        preserve_owned_annotations: true,
    };
    let result = changed(&valid);
    assert!(result.annotations[0].is_shortcut_emphasis());
    let invalid = PastePayload {
        content,
        annotations: vec![ContentAnnotation::shortcut(start + 1, usize::MAX)],
        verified_paths: Vec::new(),
        preserve_owned_annotations: true,
    };
    assert!(invalid.reflow().is_err());
}

#[test]
fn partial_large_fold_never_absorbs_neighboring_text() {
    let prefix = "prefix:";
    let large = format!("{}\n{}", "alpha ".repeat(300), "beta ".repeat(300));
    let suffix = ":suffix";
    let content = format!("{prefix}{large}{suffix}");
    let start = prefix.len();
    let end = start + large.len();
    let payload = PastePayload {
        content,
        annotations: vec![ContentAnnotation {
            start,
            end,
            kind: ContentAnnotationKind::LargePaste {
                lines: 2,
                graphemes: large.graphemes(true).count(),
            },
        }],
        verified_paths: Vec::new(),
        preserve_owned_annotations: false,
    };

    let result = changed(&payload);
    assert!(result.content.starts_with(prefix));
    assert!(result.content.ends_with(suffix));
    let [fold] = result.annotations.as_slice() else {
        panic!("expected one retained fold");
    };
    assert_eq!(fold.start, prefix.len());
    assert_eq!(fold.end, result.content.len() - suffix.len());
    assert!(!result.content[fold.start..fold.end].contains('\n'));
}

#[test]
fn partial_large_fold_boundaries_receive_normal_whitespace_cleanup() {
    let prefix = "prefix";
    let large = format!("  {}\nwrapped  ", "a".repeat(1_200));
    let suffix = "suffix";
    let content = format!("{prefix}\n\n\n\n{large}\n\n\n\n{suffix}");
    let start = content.find(&large).expect("large start");
    let end = start + large.len();
    let payload = PastePayload {
        content,
        annotations: vec![ContentAnnotation {
            start,
            end,
            kind: ContentAnnotationKind::LargePaste {
                lines: 2,
                graphemes: large.graphemes(true).count(),
            },
        }],
        verified_paths: Vec::new(),
        preserve_owned_annotations: false,
    };

    let transformed = crate::ui::paste_reflow::reflow_text_isolated(
        &payload.content,
        &[],
        std::slice::from_ref(&(start..end)),
    )
    .expect("boundary reflow succeeds");
    let expected = format!("{prefix}\n\n{} wrapped\n\n{suffix}", "a".repeat(1_200));
    assert_eq!(transformed.content, expected);
    let annotations = super::reflow_annotations(&payload, &transformed)
        .expect("boundary annotations remain valid");
    let [fold] = annotations.as_slice() else {
        panic!("expected one retained fold");
    };
    assert!(!transformed.content[fold.start..fold.end].contains(prefix));
    assert!(!transformed.content[fold.start..fold.end].contains(suffix));
}

#[test]
fn many_partial_large_folds_reflow_without_crossing_boundaries() {
    let mut content = String::new();
    let mut annotations = Vec::new();
    for index in 0..64 {
        write!(content, "prefix{index}:").expect("string formatting");
        let start = content.len();
        content.push_str(&"a".repeat(600));
        content.push('\n');
        content.push_str(&"界".repeat(600));
        let end = content.len();
        annotations.push(ContentAnnotation {
            start,
            end,
            kind: ContentAnnotationKind::LargePaste {
                lines: 2,
                graphemes: 1_201,
            },
        });
        content.push_str(":suffix\n\n");
    }
    let payload = PastePayload {
        content,
        annotations,
        verified_paths: Vec::new(),
        preserve_owned_annotations: false,
    };

    let result = changed(&payload);
    assert_eq!(result.annotations.len(), 64);
    for fold in &result.annotations {
        assert!(!result.content[fold.start..fold.end].contains('\n'));
        assert!(matches!(
            fold.kind,
            ContentAnnotationKind::LargePaste {
                lines: 1,
                graphemes: 1_201
            }
        ));
        assert!(!result.content[fold.start..fold.end].contains("prefix"));
        assert!(!result.content[fold.start..fold.end].contains("suffix"));
    }
}
