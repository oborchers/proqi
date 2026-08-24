use std::path::PathBuf;

use crate::{
    adapters::memory::FakeIdGenerator,
    domain::RequestId,
    ports::{
        attachment::{AttachmentError, AttachmentStore, RasterImage},
        clipboard::{Clipboard, ClipboardContent, ClipboardError, ClipboardWrite},
        environment::IdGenerator as _,
    },
};

use super::{ExternalReadError, read_clipboard};

struct FakeClipboard(Result<ClipboardContent, ClipboardError>);

impl Clipboard for FakeClipboard {
    fn write(&mut self, _content: &str) -> Result<ClipboardWrite, ClipboardError> {
        Err(ClipboardError::Unavailable("unused".to_owned()))
    }

    fn read(&mut self) -> Result<ClipboardContent, ClipboardError> {
        self.0.clone()
    }
}

#[derive(Default)]
struct FakeAttachments {
    saved: Option<(RequestId, RasterImage)>,
    result: Option<Result<PathBuf, AttachmentError>>,
}

impl AttachmentStore for FakeAttachments {
    fn save_clipboard_image(
        &mut self,
        request_id: RequestId,
        image: &RasterImage,
    ) -> Result<PathBuf, AttachmentError> {
        self.saved = Some((request_id, image.clone()));
        self.result
            .take()
            .ok_or_else(|| AttachmentError::Io("no fake result".to_owned()))?
    }
}

#[test]
fn image_read_materializes_exact_pixels_before_returning_a_path() {
    let image = RasterImage::new(1, 1, vec![1, 2, 3, 255]).expect("image");
    let mut clipboard = FakeClipboard(Ok(ClipboardContent::Image(image.clone())));
    let path = PathBuf::from("/private/proqi/clipboard.png");
    let mut attachments = FakeAttachments {
        saved: None,
        result: Some(Ok(path.clone())),
    };
    let mut ids = FakeIdGenerator::new(1_725_000_000_000);
    let request = ids.request_id();

    assert_eq!(
        read_clipboard(&mut clipboard, &mut attachments, request),
        Ok(super::super::path_import::attachment_payload(
            path.to_string_lossy().into_owned(),
            true,
        ))
    );
    assert_eq!(attachments.saved, Some((request, image)));
}

#[test]
fn attachment_failure_returns_no_insertable_path() {
    let image = RasterImage::new(1, 1, vec![1, 2, 3, 255]).expect("image");
    let mut clipboard = FakeClipboard(Ok(ClipboardContent::Image(image)));
    let mut attachments = FakeAttachments {
        saved: None,
        result: Some(Err(AttachmentError::Io("disk full".to_owned()))),
    };
    let mut ids = FakeIdGenerator::new(1_725_000_000_000);

    assert!(matches!(
        read_clipboard(&mut clipboard, &mut attachments, ids.request_id()),
        Err(ExternalReadError::Attachment)
    ));
}
