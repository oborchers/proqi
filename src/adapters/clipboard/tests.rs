//! Multi-format clipboard identity and fallback contracts.

use std::sync::{Arc, Mutex};

use crate::{
    domain::{ContentAnnotation, ContentAnnotationKind},
    ports::{
        attachment::RasterImage,
        clipboard::{Clipboard, ClipboardContent, ClipboardError, ClipboardText, ClipboardWrite},
    },
};

use super::{
    FileClipboardProvenance, NativeClipboard, NativeReadError, PlatformClipboard, TypedSnapshot,
    decode_payload, encode_payload,
};

#[derive(Default)]
struct FakeState {
    content: Option<String>,
    typed: Option<String>,
    image: Option<RasterImage>,
    generation: u64,
    unavailable: bool,
    omit_typed: bool,
    replace_after_typed_read: bool,
}

#[derive(Clone, Default)]
struct FakeNative {
    state: Arc<Mutex<FakeState>>,
}

impl NativeClipboard for FakeNative {
    fn write_text(&mut self, content: &str) -> Result<(), String> {
        let mut state = self.state.lock().map_err(|error| error.to_string())?;
        if state.unavailable {
            return Err("unavailable".to_owned());
        }
        state.generation = state.generation.saturating_add(1);
        state.content = Some(content.to_owned());
        state.typed = None;
        Ok(())
    }

    fn write_typed(&mut self, content: &str, payload: &str) -> Result<u64, String> {
        let mut state = self.state.lock().map_err(|error| error.to_string())?;
        if state.unavailable {
            return Err("unavailable".to_owned());
        }
        state.generation = state.generation.saturating_add(1);
        state.content = Some(content.to_owned());
        state.typed = (!state.omit_typed).then(|| payload.to_owned());
        Ok(state.generation)
    }

    fn read_text(&mut self) -> Result<String, NativeReadError> {
        self.state
            .lock()
            .map_err(|error| NativeReadError::Unavailable(error.to_string()))?
            .content
            .clone()
            .ok_or_else(|| NativeReadError::Unavailable("unavailable".to_owned()))
    }

    fn read_typed(&mut self) -> Result<TypedSnapshot, String> {
        let mut state = self.state.lock().map_err(|error| error.to_string())?;
        if state.unavailable {
            return Err("unavailable".to_owned());
        }
        let snapshot = TypedSnapshot {
            generation: state.generation,
            payload: state.typed.clone(),
        };
        if state.replace_after_typed_read {
            state.replace_after_typed_read = false;
            state.generation = state.generation.saturating_add(1);
        }
        Ok(snapshot)
    }

    fn read_image(&mut self) -> Result<RasterImage, NativeReadError> {
        self.state
            .lock()
            .map_err(|error| NativeReadError::Unavailable(error.to_string()))?
            .image
            .clone()
            .ok_or_else(|| NativeReadError::Unavailable("unavailable".to_owned()))
    }
}

fn clipboard(native: FakeNative, cache: &std::path::Path) -> PlatformClipboard {
    PlatformClipboard {
        native: Box::new(native),
        provenance: FileClipboardProvenance::new(cache),
        osc52: true,
    }
}

fn request_id() -> crate::domain::RequestId {
    "req_06g30t8fudrq55fdkjqr6mpe44"
        .parse()
        .expect("request ID")
}

fn second_request_id() -> crate::domain::RequestId {
    "req_06g30t8fudrq55fdkjqr6mpe48"
        .parse()
        .expect("request ID")
}

fn attachment(content: &str) -> ClipboardText {
    ClipboardText::new(
        content.to_owned(),
        vec![ContentAnnotation {
            start: 0,
            end: content.len(),
            kind: ContentAnnotationKind::Attachment {
                image: true,
                display_name: "Grüße 🖼️.png".to_owned(),
            },
        }],
    )
    .expect("annotation")
}

#[test]
fn native_failure_returns_an_exact_bounded_osc52_sequence() {
    let root = tempfile::tempdir().expect("temporary directory");
    let native = FakeNative::default();
    native.state.lock().expect("state").unavailable = true;
    let mut clipboard = clipboard(native, root.path());
    assert_eq!(
        clipboard.write(request_id(), &ClipboardText::plain("Grüße\n".to_owned())),
        Ok(ClipboardWrite::Osc52(
            b"\x1b]52;c;R3LDvMOfZQo=\x07".to_vec()
        ))
    );
}

#[test]
fn annotated_write_requires_both_native_representations() {
    let root = tempfile::tempdir().expect("temporary directory");
    let native = FakeNative::default();
    native.state.lock().expect("state").omit_typed = true;
    let mut clipboard = clipboard(native, root.path());
    assert!(matches!(
        clipboard.write(request_id(), &attachment("/tmp/missing.png")),
        Err(ClipboardError::Unavailable(_))
    ));
}

#[test]
fn annotated_text_round_trips_across_adapter_restart() {
    let root = tempfile::tempdir().expect("temporary directory");
    let expected = attachment("/Volumes/offline/Grüße 🖼️.png");
    let native = FakeNative::default();
    let mut writer = clipboard(native.clone(), root.path());
    assert_eq!(
        writer.write(request_id(), &expected),
        Ok(ClipboardWrite::Native)
    );
    drop(writer);

    let mut reader = clipboard(native, root.path());
    assert_eq!(
        reader.read().expect("clipboard"),
        ClipboardContent::Text(expected)
    );
}

#[test]
fn payload_decodes_only_for_its_exact_text() {
    let expected = attachment("/tmp/<same>&.png");
    let encoded = encode_payload(request_id(), &expected).expect("encode");
    assert_eq!(
        decode_payload(expected.content(), &encoded.payload).map(|(text, _, _)| text),
        Some(expected)
    );
    assert!(decode_payload("/tmp/<same>&.png changed", &encoded.payload).is_none());
    assert!(decode_payload("/tmp/<same>&.png", "not-base64").is_none());
}

#[test]
fn identical_plain_text_replacement_cannot_reuse_stale_metadata() {
    let root = tempfile::tempdir().expect("temporary directory");
    let expected = attachment("/tmp/repeated.png");
    let native = FakeNative::default();
    let mut clipboard = clipboard(native.clone(), root.path());
    clipboard.write(request_id(), &expected).expect("write");
    {
        let mut state = native.state.lock().expect("state");
        state.generation = state.generation.saturating_add(1);
        state.content = Some(expected.content().to_owned());
        assert!(
            state.typed.is_some(),
            "retain the deliberately stale flavor"
        );
    }
    assert_eq!(
        clipboard.read().expect("plain replacement"),
        ClipboardContent::Text(ClipboardText::plain(expected.content().to_owned()))
    );
}

#[test]
fn externally_forged_valid_metadata_cannot_bypass_provenance() {
    let root = tempfile::tempdir().expect("temporary directory");
    let original = attachment("/tmp/repeated.png");
    let forged = ClipboardText::new(
        original.content().to_owned(),
        vec![ContentAnnotation::shortcut(0, original.content().len())],
    )
    .expect("forged annotation is structurally valid");
    let native = FakeNative::default();
    let mut clipboard = clipboard(native.clone(), root.path());
    clipboard.write(request_id(), &original).expect("write");
    let forged_payload = encode_payload(second_request_id(), &forged).expect("encode");
    {
        let mut state = native.state.lock().expect("state");
        state.generation = state.generation.saturating_add(1);
        state.content = Some(forged.content().to_owned());
        state.typed = Some(forged_payload.payload);
    }
    assert_eq!(
        clipboard.read().expect("clipboard"),
        ClipboardContent::Text(ClipboardText::plain(forged.content().to_owned()))
    );
}

#[test]
fn replacement_during_read_cannot_mix_typed_and_plain_snapshots() {
    let root = tempfile::tempdir().expect("temporary directory");
    let expected = attachment("/tmp/repeated.png");
    let native = FakeNative::default();
    let mut clipboard = clipboard(native.clone(), root.path());
    clipboard.write(request_id(), &expected).expect("write");
    native.state.lock().expect("state").replace_after_typed_read = true;
    assert_eq!(
        clipboard.read().expect("clipboard"),
        ClipboardContent::Text(ClipboardText::plain(expected.content().to_owned()))
    );
}

#[test]
fn malformed_private_provenance_fails_closed_to_plain_text() {
    let root = tempfile::tempdir().expect("temporary directory");
    let expected = attachment("/tmp/repeated.png");
    let native = FakeNative::default();
    let mut clipboard = clipboard(native, root.path());
    clipboard.write(request_id(), &expected).expect("write");
    std::fs::write(root.path().join("clipboard/provenance.json"), b"not-json")
        .expect("corrupt provenance");
    assert_eq!(
        clipboard.read().expect("clipboard"),
        ClipboardContent::Text(ClipboardText::plain(expected.content().to_owned()))
    );
}

#[cfg(unix)]
#[test]
fn unsafe_provenance_path_rejects_annotated_write_before_clipboard_mutation() {
    use std::os::unix::fs::symlink;

    let root = tempfile::tempdir().expect("temporary directory");
    let target = tempfile::tempdir().expect("target directory");
    symlink(target.path(), root.path().join("clipboard")).expect("symlink");
    let native = FakeNative::default();
    native.state.lock().expect("state").content = Some("before".to_owned());
    let mut clipboard = clipboard(native.clone(), root.path());
    assert!(matches!(
        clipboard.write(request_id(), &attachment("/tmp/repeated.png")),
        Err(ClipboardError::Unavailable(_))
    ));
    assert_eq!(
        native.state.lock().expect("state").content.as_deref(),
        Some("before")
    );
}

#[test]
fn successful_native_read_preserves_exact_text() {
    let root = tempfile::tempdir().expect("temporary directory");
    let native = FakeNative::default();
    native.state.lock().expect("state").content = Some(" exact\r\n".to_owned());
    let mut clipboard = clipboard(native, root.path());
    assert_eq!(
        clipboard.read().expect("clipboard"),
        ClipboardContent::Text(ClipboardText::plain(" exact\r\n".to_owned()))
    );
}

#[test]
fn native_image_is_preferred_and_remains_exact_rgba() {
    let root = tempfile::tempdir().expect("temporary directory");
    let image = RasterImage::new(1, 1, vec![1, 2, 3, 255]).expect("image");
    let native = FakeNative::default();
    {
        let mut state = native.state.lock().expect("state");
        state.content = Some("fallback text".to_owned());
        state.image = Some(image.clone());
    }
    let mut clipboard = clipboard(native, root.path());
    assert_eq!(
        clipboard.read().expect("clipboard"),
        ClipboardContent::Image(image)
    );
}
