//! RAII update lock ownership.

use std::fs::File;

use fs4::FileExt;

use crate::ports::update::UpdateLease;

pub(super) struct FileUpdateLease {
    pub(super) file: File,
}

impl UpdateLease for FileUpdateLease {}

impl Drop for FileUpdateLease {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.file);
    }
}
