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
fn protected_whitespace_and_line_delimiters_are_never_reflowed_away() {
    for (content, start, end) in [
        (" ", 0, 1),
        ("\t", 0, 1),
        ("first\nsecond", 5, 6),
        ("first\r\nsecond", 5, 7),
    ] {
        let annotation = ContentAnnotation {
            start,
            end,
            kind: ContentAnnotationKind::InvocationReference {
                display_name: "@agent · codex".to_owned(),
            },
        };
        let payload = PastePayload::preserved_clipboard(content.to_owned(), vec![annotation])
            .expect("valid whitespace annotation");
        assert!(
            matches!(payload.reflow(), Ok(PasteReflow::Unchanged)),
            "content {content:?}"
        );
    }
}

#[test]
fn unchanged_text_still_recomputes_or_drops_stale_large_fold_metadata() {
    for (content, expected) in [
        ("a".repeat(1_200), Some((1, 1_200))),
        ("short/path".to_owned(), None),
    ] {
        let payload = PastePayload {
            annotations: vec![ContentAnnotation {
                start: 0,
                end: content.len(),
                kind: ContentAnnotationKind::LargePaste {
                    lines: 99,
                    graphemes: 99,
                },
            }],
            content: content.clone(),
            verified_paths: Vec::new(),
            preserve_owned_annotations: false,
        };
        let result = changed(&payload);
        assert_eq!(result.content, content);
        match expected {
            Some((lines, graphemes)) => assert!(matches!(
                result.annotations.as_slice(),
                [ContentAnnotation {
                    start: 0,
                    end,
                    kind: ContentAnnotationKind::LargePaste {
                        lines: actual_lines,
                        graphemes: actual_graphemes,
                    },
                }] if *end == result.content.len()
                    && *actual_lines == lines
                    && *actual_graphemes == graphemes
            )),
            None => assert!(result.annotations.is_empty()),
        }
    }
}

#[test]
fn protected_boundary_whitespace_survives_next_to_an_isolated_large_fold() {
    let prefix = " \t\r\n";
    let large = "a".repeat(1_200);
    let suffix = "\r\n \t";
    let content = format!("{prefix}{large}{suffix}");
    let large_start = prefix.len();
    let large_end = large_start + large.len();
    let payload = PastePayload {
        content: content.clone(),
        annotations: vec![
            ContentAnnotation {
                start: 0,
                end: prefix.len(),
                kind: ContentAnnotationKind::InvocationReference {
                    display_name: "@prefix · codex".to_owned(),
                },
            },
            ContentAnnotation {
                start: large_start,
                end: large_end,
                kind: ContentAnnotationKind::LargePaste {
                    lines: 99,
                    graphemes: 99,
                },
            },
            ContentAnnotation::shortcut(large_end, content.len()),
        ],
        verified_paths: Vec::new(),
        preserve_owned_annotations: true,
    };

    let result = changed(&payload);
    assert_eq!(result.content, content);
    assert_eq!(&result.content[..prefix.len()], prefix);
    assert_eq!(&result.content[large_start..large_end], large);
    assert_eq!(&result.content[large_end..], suffix);
    assert_eq!(result.annotations[0], payload.annotations[0]);
    assert_eq!(result.annotations[2], payload.annotations[2]);
    assert!(matches!(
        result.annotations[1].kind,
        ContentAnnotationKind::LargePaste {
            lines: 1,
            graphemes: 1_200
        }
    ));
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
    assert_eq!(&result.content[fold.end..], format!(" {suffix}"));
    assert!(!result.content[fold.start..fold.end].contains('\n'));
    assert!(!result.content[fold.start..fold.end].ends_with(' '));
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
    assert_eq!(
        &transformed.content[fold.start..fold.end],
        format!("{} wrapped", "a".repeat(1_200))
    );
    assert_eq!(&transformed.content[..fold.start], format!("{prefix}\n\n"));
    assert_eq!(&transformed.content[fold.end..], format!("\n\n{suffix}"));
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
