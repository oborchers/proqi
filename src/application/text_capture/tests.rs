//! Source precedence, acceptance, and feedback of explicit text capture.

use crate::{
    adapters::memory::ScriptedClipboard,
    ports::{
        attachment::RasterImage,
        clipboard::{ClipboardContent, ClipboardError, ClipboardText},
    },
};

use super::{
    CaptureError, CaptureSource, CapturedText, MAX_THOUGHT_INPUT_BYTES, capture_text,
    clipboard_text,
};

#[test]
fn a_nonempty_selection_wins_without_reading_the_clipboard() {
    let mut clipboard = ScriptedClipboard::text("clipboard");
    let captured = capture_text(Some("  selected\r\n".to_owned()), &mut clipboard).expect("text");
    assert_eq!(captured.source(), CaptureSource::Selection);
    assert_eq!(captured.text(), "  selected\r\n");
    assert_eq!(clipboard.reads, 0);
}

#[test]
fn a_missing_or_empty_selection_falls_back_to_the_clipboard() {
    for selection in [None, Some(String::new())] {
        let mut clipboard = ScriptedClipboard::text("from clipboard\n");
        let captured = capture_text(selection, &mut clipboard).expect("text");
        assert_eq!(captured.source(), CaptureSource::Clipboard);
        assert_eq!(captured.text(), "from clipboard\n");
        assert_eq!(clipboard.reads, 1);
    }
}

#[test]
fn a_whitespace_selection_is_rejected_instead_of_reading_the_clipboard() {
    let mut clipboard = ScriptedClipboard::text("clipboard");
    let error = capture_text(Some(" \n\t".to_owned()), &mut clipboard).expect_err("empty");
    assert_eq!(
        error,
        CaptureError::Empty {
            source: CaptureSource::Selection
        }
    );
    assert_eq!(
        error.to_string(),
        "Nothing captured: the selection is empty"
    );
    assert_eq!(clipboard.reads, 0);
}

#[test]
fn empty_and_whitespace_clipboard_text_is_rejected() {
    for text in ["", "   ", "\n\r\n\t"] {
        let error = clipboard_text(&mut ScriptedClipboard::text(text)).expect_err("empty");
        assert_eq!(
            error.to_string(),
            "Nothing captured: the clipboard is empty"
        );
    }
}

#[test]
fn non_text_clipboard_content_reports_that_there_is_no_text() {
    let image = RasterImage::new(1, 1, vec![0; 4]).expect("image");
    for content in [
        Ok(ClipboardContent::Image(image)),
        Err(ClipboardError::InvalidImage),
        Err(ClipboardError::InvalidText),
    ] {
        let error = clipboard_text(&mut ScriptedClipboard::with(content)).expect_err("no text");
        assert_eq!(error, CaptureError::NoText);
        assert_eq!(
            error.to_string(),
            "Nothing captured: the clipboard holds no text (for example an image)"
        );
    }
}

#[test]
fn an_unreadable_clipboard_keeps_its_typed_cause() {
    let error = clipboard_text(&mut ScriptedClipboard::with(Err(ClipboardError::TimedOut)))
        .expect_err("failure");
    assert_eq!(error, CaptureError::Clipboard(ClipboardError::TimedOut));
}

#[test]
fn the_size_limit_is_inclusive_and_never_truncates() {
    let limit = "a".repeat(MAX_THOUGHT_INPUT_BYTES);
    let accepted = CapturedText::new(CaptureSource::Clipboard, limit.clone()).expect("limit");
    assert_eq!(accepted.text(), limit);
    let over = format!("{limit}é");
    let error = CapturedText::new(CaptureSource::Clipboard, over.clone()).expect_err("over");
    assert_eq!(
        error,
        CaptureError::TooLarge {
            source: CaptureSource::Clipboard,
            bytes: over.len()
        }
    );
    assert!(error.to_string().contains("131074 bytes"));
}

#[test]
fn clipboard_annotations_are_dropped_and_text_stays_exact() {
    let annotated = ClipboardText::new(
        "/tmp/a.png tail".to_owned(),
        vec![crate::domain::ContentAnnotation {
            start: 0,
            end: 10,
            kind: crate::domain::ContentAnnotationKind::Attachment {
                ordinal: None,
                image: true,
                display_name: "a.png".to_owned(),
            },
        }],
    )
    .expect("annotated");
    let captured = clipboard_text(&mut ScriptedClipboard::with(Ok(ClipboardContent::Text(
        annotated,
    ))))
    .expect("text");
    assert_eq!(captured.text(), "/tmp/a.png tail");
}

#[test]
fn preview_is_one_control_free_line_with_collapsed_whitespace() {
    let captured = CapturedText::new(
        CaptureSource::Selection,
        "\n  line one\r\n\tline\u{1b}[31m two\u{7}  \n".to_owned(),
    )
    .expect("text");
    assert_eq!(captured.preview(), "line one line[31m two");
}

#[test]
fn preview_truncates_graphemes_with_an_ellipsis_and_counts_perceived_characters() {
    let text = "👩‍👩‍👧é".repeat(30);
    let captured = CapturedText::new(CaptureSource::Clipboard, text).expect("text");
    let preview = captured.preview();
    assert!(preview.ends_with('…'));
    assert_eq!(
        unicode_segmentation::UnicodeSegmentation::graphemes(preview.as_str(), true).count(),
        41
    );
    assert_eq!(captured.characters(), 60);
    assert_eq!(
        captured.confirmation(),
        format!("Captured to Proqi: \"{preview}\" (60 characters)")
    );
}

#[test]
fn confirmation_uses_the_singular_for_one_character() {
    let captured = CapturedText::new(CaptureSource::Clipboard, "x".to_owned()).expect("text");
    assert_eq!(
        captured.confirmation(),
        "Captured to Proqi: \"x\" (1 character)"
    );
}

#[test]
fn debug_output_never_contains_the_captured_text() {
    let captured = CapturedText::new(CaptureSource::Clipboard, "secret".to_owned()).expect("text");
    assert!(!format!("{captured:?}").contains("secret"));
}

#[test]
fn preview_drops_bidirectional_formatting_that_could_reorder_the_notification() {
    let captured = CapturedText::new(
        CaptureSource::Selection,
        "safe \u{202E}txt.exe\u{202C} \u{2066}iso\u{2069}\u{061C}".to_owned(),
    )
    .expect("text");
    assert_eq!(captured.preview(), "safe txt.exe iso");
    assert_eq!(
        captured.text(),
        "safe \u{202E}txt.exe\u{202C} \u{2066}iso\u{2069}\u{061C}",
        "stored content stays exact"
    );
}
