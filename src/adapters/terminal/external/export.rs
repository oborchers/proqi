//! Plain-text export writes and destination listings on the external lane.

use std::path::PathBuf;

use crate::{
    adapters::export::FileExport,
    domain::RequestId,
    ports::export::{
        DirectoryLister as _, DirectoryListing, DirectoryListingError, ExportWriteError,
        ExportWriteRequest, ExportWriter as _, ExportWritten,
    },
};

use super::ExternalResult;

/// Plain-text export work owned by the external lane.
pub(super) enum ThoughtExportRequest {
    Write {
        request_id: RequestId,
        request: Box<ExportWriteRequest>,
    },
    List {
        generation: u64,
        directory: PathBuf,
    },
}

/// Completion of one plain-text export request.
pub(in crate::adapters::terminal) enum ThoughtExportResult {
    Written {
        request_id: RequestId,
        result: Result<ExportWritten, ExportWriteError>,
    },
    Listed {
        generation: u64,
        result: Result<DirectoryListing, DirectoryListingError>,
    },
}

pub(super) fn run_thought_export(request: ThoughtExportRequest) -> ExternalResult {
    ExternalResult::ThoughtExport(match request {
        ThoughtExportRequest::Write {
            request_id,
            request,
        } => ThoughtExportResult::Written {
            request_id,
            result: FileExport.write(&request),
        },
        ThoughtExportRequest::List {
            generation,
            directory,
        } => ThoughtExportResult::Listed {
            generation,
            result: FileExport.list(&directory),
        },
    })
}
