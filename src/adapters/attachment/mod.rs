//! Atomic private PNG materialization for clipboard images.

mod accessibility;

pub use accessibility::FileAttachmentAccessibility;

use std::{
    fs::{self, File, OpenOptions},
    io::BufWriter,
    path::{Path, PathBuf},
};

use crate::{
    domain::RequestId,
    ports::attachment::{AttachmentError, AttachmentStore, RasterImage},
};

/// Filesystem attachment store scoped to one Proqi session.
pub struct FileAttachmentStore {
    directory: PathBuf,
}

impl FileAttachmentStore {
    /// Construct a store for one absolute session attachment directory.
    #[must_use]
    pub fn new(directory: PathBuf) -> Self {
        Self { directory }
    }
}

impl AttachmentStore for FileAttachmentStore {
    fn save_clipboard_image(
        &mut self,
        request_id: RequestId,
        image: &RasterImage,
    ) -> Result<PathBuf, AttachmentError> {
        let root = self
            .directory
            .parent()
            .ok_or_else(|| invalid_directory(&self.directory))?;
        prepare_directory(root)?;
        prepare_directory(&self.directory)?;
        let stem = format!("clipboard-{request_id}");
        let temporary = self.directory.join(format!(".{stem}.tmp"));
        let destination = self.directory.join(format!("{stem}.png"));
        refuse_existing(&temporary)?;
        refuse_existing(&destination)?;
        let result = encode_and_install(image, &temporary, &destination, &self.directory);
        if result.is_err() {
            let _cleanup = fs::remove_file(&temporary);
        }
        result.map(|()| destination)
    }
}

fn prepare_directory(path: &Path) -> Result<(), AttachmentError> {
    if !path.is_absolute() {
        return Err(invalid_directory(path));
    }
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            return Err(invalid_directory(path));
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir_all(path).map_err(io_error)?;
        }
        Err(error) => return Err(io_error(error)),
    }
    set_private_directory(path)
}

fn refuse_existing(path: &Path) -> Result<(), AttachmentError> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Ok(_) => Err(AttachmentError::Io(format!(
            "attachment target already exists: {}",
            path.display()
        ))),
        Err(error) => Err(io_error(error)),
    }
}

fn encode_and_install(
    image: &RasterImage,
    temporary: &Path,
    destination: &Path,
    directory: &Path,
) -> Result<(), AttachmentError> {
    let file = create_private_file(temporary)?;
    encode_png(&file, image)?;
    file.sync_all().map_err(io_error)?;
    fs::rename(temporary, destination).map_err(io_error)?;
    sync_directory(directory)
}

fn encode_png(file: &File, image: &RasterImage) -> Result<(), AttachmentError> {
    let mut encoder = png::Encoder::new(BufWriter::new(file), image.width(), image.height());
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder
        .write_header()
        .map_err(|error| AttachmentError::Encoding(error.to_string()))?;
    writer
        .write_image_data(image.rgba())
        .map_err(|error| AttachmentError::Encoding(error.to_string()))?;
    writer
        .finish()
        .map_err(|error| AttachmentError::Encoding(error.to_string()))
}

fn sync_directory(path: &Path) -> Result<(), AttachmentError> {
    File::open(path)
        .and_then(|handle| handle.sync_all())
        .map_err(io_error)
}

fn create_private_file(path: &Path) -> Result<File, AttachmentError> {
    use std::os::unix::fs::OpenOptionsExt;
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .map_err(io_error)
}

fn set_private_directory(path: &Path) -> Result<(), AttachmentError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(io_error)
}

fn invalid_directory(path: &Path) -> AttachmentError {
    AttachmentError::InvalidDirectory(path.display().to_string())
}

fn io_error(error: std::io::Error) -> AttachmentError {
    let message = error.to_string();
    drop(error);
    AttachmentError::Io(message)
}

#[cfg(test)]
mod tests {
    use std::io::BufReader;

    use crate::{
        adapters::memory::FakeIdGenerator,
        ports::{
            attachment::{AttachmentStore, RasterImage},
            environment::IdGenerator as _,
        },
    };

    use super::FileAttachmentStore;

    #[test]
    fn rgba_is_atomically_encoded_to_a_private_png() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let root = temporary.path().join("attachments");
        let directory = root.join("ses_06g30t7dv5qv55n1ppn3clis3k");
        let mut store = FileAttachmentStore::new(directory.clone());
        let image = RasterImage::new(2, 1, vec![255, 0, 0, 255, 0, 255, 0, 255]).expect("image");
        let mut ids = FakeIdGenerator::new(1_725_000_000_000);
        let path = store
            .save_clipboard_image(ids.request_id(), &image)
            .expect("saved image");
        assert!(path.is_absolute());
        assert_eq!(path.parent(), Some(directory.as_path()));

        let decoder = png::Decoder::new(BufReader::new(
            std::fs::File::open(&path).expect("PNG file"),
        ));
        let mut reader = decoder.read_info().expect("PNG header");
        let mut pixels = vec![0; reader.output_buffer_size().expect("buffer size")];
        let info = reader.next_frame(&mut pixels).expect("PNG frame");
        assert_eq!((info.width, info.height), (2, 1));
        assert_eq!(&pixels[..info.buffer_size()], image.rgba());

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            assert_eq!(
                std::fs::metadata(&path)
                    .expect("metadata")
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
            assert_eq!(
                std::fs::metadata(&root)
                    .expect("root metadata")
                    .permissions()
                    .mode()
                    & 0o777,
                0o700
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_attachment_directory_is_refused() {
        use std::os::unix::fs::symlink;

        let temporary = tempfile::tempdir().expect("temporary directory");
        let target = temporary.path().join("target");
        std::fs::create_dir(&target).expect("target");
        let linked_root = temporary.path().join("linked");
        symlink(&target, &linked_root).expect("symlink");
        let mut store = FileAttachmentStore::new(linked_root.join("session"));
        let image = RasterImage::new(1, 1, vec![0, 0, 0, 255]).expect("image");
        let mut ids = FakeIdGenerator::new(1_725_000_000_000);
        assert!(
            store
                .save_clipboard_image(ids.request_id(), &image)
                .is_err()
        );
    }
}
