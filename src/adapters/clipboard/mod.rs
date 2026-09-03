//! Native system clipboard with an OSC 52 write fallback.

#[cfg(target_os = "macos")]
mod macos;
mod provenance;

use std::path::Path;

use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use crate::{
    domain::{RequestId, validate_annotations},
    ports::{
        attachment::RasterImage,
        clipboard::{Clipboard, ClipboardContent, ClipboardError, ClipboardText, ClipboardWrite},
        environment::ProcessRunner,
    },
};

use provenance::{FileClipboardProvenance, ProvenanceRecord};

const OSC52_MAX_BYTES: usize = 100_000;
const METADATA_MAX_BYTES: usize = 512 * 1024;
const WIRE_SCHEMA_VERSION: u8 = 1;

/// Native clipboard adapter with a bounded terminal fallback.
pub struct PlatformClipboard {
    native: Box<dyn NativeClipboard + Send>,
    provenance: FileClipboardProvenance,
    osc52: bool,
}

impl PlatformClipboard {
    /// Enable native access, generation-bound provenance, and the terminal fallback.
    #[must_use]
    pub fn new(cache_directory: &Path, runner: Box<dyn ProcessRunner + Send>) -> Self {
        Self {
            native: Box::new(ArboardNative::new(platform_typed_clipboard(runner))),
            provenance: FileClipboardProvenance::new(cache_directory),
            osc52: true,
        }
    }

    /// Disable OSC 52 for terminals whose policy forbids it.
    #[must_use]
    pub fn without_osc52(mut self) -> Self {
        self.osc52 = false;
        self
    }
}

impl Clipboard for PlatformClipboard {
    fn write(
        &mut self,
        request_id: RequestId,
        content: &ClipboardText,
    ) -> Result<ClipboardWrite, ClipboardError> {
        if content.annotations().is_empty() && self.native.write_text(content.content()).is_ok() {
            return Ok(ClipboardWrite::Native);
        }
        if !content.annotations().is_empty() {
            let encoded = encode_payload(request_id, content)?;
            let lease = self
                .provenance
                .acquire()
                .map_err(ClipboardError::Unavailable)?;
            let generation = self
                .native
                .write_typed(content.content(), &encoded.payload)?;
            let record = ProvenanceRecord::new(generation, encoded.request_id, encoded.binding);
            lease.store(&record).map_err(ClipboardError::Unavailable)?;
            if self.typed_write_matches(generation, content.content(), &encoded.payload) {
                return Ok(ClipboardWrite::Native);
            }
            return Err(ClipboardError::Unavailable(
                "native provider did not retain both clipboard representations".to_owned(),
            ));
        }
        if !self.osc52 {
            return Err(ClipboardError::Unavailable(
                "native providers failed and OSC 52 is disabled".to_owned(),
            ));
        }
        osc52(content.content()).map(ClipboardWrite::Osc52)
    }

    fn read(&mut self) -> Result<ClipboardContent, ClipboardError> {
        match self.native.read_image() {
            Ok(image) => return Ok(ClipboardContent::Image(image)),
            Err(NativeReadError::InvalidImage) => return Err(ClipboardError::InvalidImage),
            Err(NativeReadError::Unavailable(_)) => {}
        }
        let lease = self.provenance.acquire().ok();
        let first_typed = lease.as_ref().and_then(|_| self.native.read_typed().ok());
        let text = self.native.read_text().map_err(|error| match error {
            NativeReadError::Unavailable(message) => ClipboardError::Unavailable(message),
            NativeReadError::InvalidImage => ClipboardError::InvalidImage,
        })?;
        let second_typed = lease.as_ref().and_then(|_| self.native.read_typed().ok());
        let provenance = lease.as_ref().and_then(|lease| lease.load().ok().flatten());
        let content = verified_typed_text(
            text,
            first_typed,
            second_typed.as_ref(),
            provenance.as_ref(),
        );
        Ok(ClipboardContent::Text(content))
    }
}

impl PlatformClipboard {
    fn typed_write_matches(
        &mut self,
        generation: u64,
        expected_text: &str,
        expected_payload: &str,
    ) -> bool {
        let Ok(first) = self.native.verify_typed(generation, expected_payload) else {
            return false;
        };
        let Ok(text) = self.native.read_text() else {
            return false;
        };
        let Ok(second) = self.native.verify_typed(generation, expected_payload) else {
            return false;
        };
        first == second
            && first.generation == generation
            && first.stable
            && first.generation_matches
            && first.payload_matches
            && text == expected_text
    }
}

trait NativeClipboard {
    fn write_text(&mut self, content: &str) -> Result<(), String>;
    fn write_typed(&mut self, content: &str, payload: &str) -> Result<u64, ClipboardError>;
    fn read_text(&mut self) -> Result<String, NativeReadError>;
    fn read_typed(&mut self) -> Result<TypedSnapshot, String>;
    fn verify_typed(&mut self, generation: u64, payload: &str)
    -> Result<TypedVerification, String>;
    fn read_image(&mut self) -> Result<RasterImage, NativeReadError>;
}

trait TypedClipboard {
    fn write(&mut self, text: &str, typed: &str) -> Result<u64, ClipboardError>;
    fn read(&mut self) -> Result<TypedSnapshot, String>;
    fn verify(&mut self, generation: u64, payload: &str) -> Result<TypedVerification, String>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct TypedSnapshot {
    generation: u64,
    payload: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct TypedVerification {
    generation: u64,
    stable: bool,
    generation_matches: bool,
    payload_matches: bool,
}

enum NativeReadError {
    Unavailable(String),
    InvalidImage,
}

struct ArboardNative {
    clipboard: Option<arboard::Clipboard>,
    typed: Box<dyn TypedClipboard + Send>,
}

impl ArboardNative {
    fn new(typed: Box<dyn TypedClipboard + Send>) -> Self {
        Self {
            clipboard: None,
            typed,
        }
    }

    fn clipboard(&mut self) -> Result<&mut arboard::Clipboard, String> {
        if self.clipboard.is_none() {
            self.clipboard = Some(arboard::Clipboard::new().map_err(|error| error.to_string())?);
        }
        self.clipboard
            .as_mut()
            .ok_or_else(|| "native clipboard initialization failed".to_owned())
    }
}

impl NativeClipboard for ArboardNative {
    fn write_text(&mut self, content: &str) -> Result<(), String> {
        self.clipboard()?
            .set_text(content.to_owned())
            .map_err(|error| error.to_string())
    }

    fn write_typed(&mut self, content: &str, payload: &str) -> Result<u64, ClipboardError> {
        self.typed.write(content, payload)
    }

    fn read_text(&mut self) -> Result<String, NativeReadError> {
        self.clipboard()
            .map_err(NativeReadError::Unavailable)?
            .get_text()
            .map_err(native_unavailable)
    }

    fn read_typed(&mut self) -> Result<TypedSnapshot, String> {
        self.typed.read()
    }

    fn verify_typed(
        &mut self,
        generation: u64,
        payload: &str,
    ) -> Result<TypedVerification, String> {
        self.typed.verify(generation, payload)
    }

    fn read_image(&mut self) -> Result<RasterImage, NativeReadError> {
        let image = self
            .clipboard()
            .map_err(NativeReadError::Unavailable)?
            .get_image()
            .map_err(native_unavailable)?;
        RasterImage::new(image.width, image.height, image.bytes.into_owned())
            .map_err(|_| NativeReadError::InvalidImage)
    }
}

#[cfg(target_os = "macos")]
fn platform_typed_clipboard(
    runner: Box<dyn ProcessRunner + Send>,
) -> Box<dyn TypedClipboard + Send> {
    Box::new(macos::MacTypedClipboard::new(runner))
}

#[cfg(not(target_os = "macos"))]
fn platform_typed_clipboard(
    _runner: Box<dyn ProcessRunner + Send>,
) -> Box<dyn TypedClipboard + Send> {
    Box::new(UnsupportedTypedClipboard)
}

#[cfg(not(target_os = "macos"))]
struct UnsupportedTypedClipboard;

#[cfg(not(target_os = "macos"))]
impl TypedClipboard for UnsupportedTypedClipboard {
    fn write(&mut self, _text: &str, _typed: &str) -> Result<u64, ClipboardError> {
        Err(ClipboardError::MetadataUnsupported)
    }

    fn read(&mut self) -> Result<TypedSnapshot, String> {
        Err("safe multi-format clipboard identity is unavailable on this platform".to_owned())
    }

    fn verify(&mut self, _generation: u64, _payload: &str) -> Result<TypedVerification, String> {
        Err("safe multi-format clipboard identity is unavailable on this platform".to_owned())
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct WirePayload {
    schema_version: u8,
    request_id: String,
    binding: [u8; 32],
    annotations: Vec<crate::domain::ContentAnnotation>,
}

struct EncodedPayload {
    payload: String,
    request_id: String,
    binding: [u8; 32],
}

fn encode_payload(
    request_id: RequestId,
    content: &ClipboardText,
) -> Result<EncodedPayload, ClipboardError> {
    let request_id = request_id.to_string();
    let binding = payload_binding(&request_id, content.content(), content.annotations())?;
    let wire = WirePayload {
        schema_version: WIRE_SCHEMA_VERSION,
        request_id: request_id.clone(),
        binding,
        annotations: content.annotations().to_vec(),
    };
    let encoded = serde_json::to_vec(&wire)
        .map_err(|error| ClipboardError::Unavailable(error.to_string()))?;
    if encoded.len() > METADATA_MAX_BYTES {
        return Err(ClipboardError::TooLarge);
    }
    Ok(EncodedPayload {
        payload: STANDARD.encode(encoded),
        request_id,
        binding,
    })
}

fn decode_payload(content: &str, encoded: &str) -> Option<(ClipboardText, String, [u8; 32])> {
    if encoded.len() > METADATA_MAX_BYTES.saturating_mul(2) {
        return None;
    }
    let bytes = STANDARD.decode(encoded).ok()?;
    if bytes.len() > METADATA_MAX_BYTES {
        return None;
    }
    let wire: WirePayload = serde_json::from_slice(&bytes).ok()?;
    if wire.schema_version != WIRE_SCHEMA_VERSION {
        return None;
    }
    let _: RequestId = wire.request_id.parse().ok()?;
    validate_annotations(content, &wire.annotations).ok()?;
    let binding = payload_binding(&wire.request_id, content, &wire.annotations).ok()?;
    if binding != wire.binding {
        return None;
    }
    ClipboardText::new(content.to_owned(), wire.annotations)
        .ok()
        .map(|text| (text, wire.request_id, wire.binding))
}

fn verified_typed_text(
    text: String,
    first: Option<TypedSnapshot>,
    second: Option<&TypedSnapshot>,
    provenance: Option<&ProvenanceRecord>,
) -> ClipboardText {
    let Some(snapshot) = first.filter(|snapshot| Some(snapshot) == second) else {
        return ClipboardText::plain(text);
    };
    let Some(payload) = snapshot.payload.as_deref() else {
        return ClipboardText::plain(text);
    };
    let Some((typed, request_id, binding)) = decode_payload(&text, payload) else {
        return ClipboardText::plain(text);
    };
    let expected = ProvenanceRecord::new(snapshot.generation, request_id, binding);
    if provenance == Some(&expected) {
        typed
    } else {
        ClipboardText::plain(text)
    }
}

fn payload_binding(
    request_id: &str,
    content: &str,
    annotations: &[crate::domain::ContentAnnotation],
) -> Result<[u8; 32], ClipboardError> {
    let annotations = serde_json::to_vec(annotations)
        .map_err(|error| ClipboardError::Unavailable(error.to_string()))?;
    let mut digest = Sha256::new();
    digest.update(b"proqi-clipboard-v1\0");
    digest.update(wire_length(request_id.len())?);
    digest.update(request_id.as_bytes());
    digest.update(wire_length(content.len())?);
    digest.update(content.as_bytes());
    digest.update(wire_length(annotations.len())?);
    digest.update(annotations);
    Ok(digest.finalize().into())
}

fn wire_length(length: usize) -> Result<[u8; 8], ClipboardError> {
    u64::try_from(length)
        .map(u64::to_le_bytes)
        .map_err(|_| ClipboardError::TooLarge)
}

fn native_unavailable(error: impl ToString) -> NativeReadError {
    let message = error.to_string();
    drop(error);
    NativeReadError::Unavailable(message)
}

fn osc52(content: &str) -> Result<Vec<u8>, ClipboardError> {
    if content.len() > OSC52_MAX_BYTES {
        return Err(ClipboardError::TooLarge);
    }
    let encoded = STANDARD.encode(content.as_bytes());
    let mut sequence = Vec::with_capacity(encoded.len().saturating_add(8));
    sequence.extend_from_slice(b"\x1b]52;c;");
    sequence.extend_from_slice(encoded.as_bytes());
    sequence.push(0x07);
    Ok(sequence)
}

#[cfg(test)]
mod tests;
