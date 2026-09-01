//! Private JSON protocol for killable attachment filesystem batches.

use std::{
    io::{Read as _, Write as _},
    path::Path,
    process::ExitCode,
};

use serde::{Deserialize, Serialize};

use crate::ports::attachment_accessibility::{AttachmentAccessFailure, AttachmentAccessibility};

use super::FileAttachmentAccessibility;

const PROTOCOL_VERSION: u8 = 1;
const MAX_REQUEST_BYTES: u64 = 1024 * 1024;
const MAX_BATCH_PATHS: usize = 32;

#[derive(Deserialize, Serialize)]
struct WorkerRequest {
    version: u8,
    paths: Vec<String>,
}

#[derive(Deserialize, Serialize)]
struct WorkerResponse {
    version: u8,
    failures: Vec<Option<String>>,
}

pub(crate) fn encode_request(paths: Vec<String>) -> Result<Vec<u8>, serde_json::Error> {
    serde_json::to_vec(&WorkerRequest {
        version: PROTOCOL_VERSION,
        paths,
    })
}

pub(crate) fn decode_response(
    bytes: &[u8],
    expected: usize,
) -> Result<Vec<Result<(), AttachmentAccessFailure>>, ()> {
    let response: WorkerResponse = serde_json::from_slice(bytes).map_err(|_| ())?;
    if response.version != PROTOCOL_VERSION || response.failures.len() != expected {
        return Err(());
    }
    response
        .failures
        .into_iter()
        .map(|failure| match failure {
            None => Ok(Ok(())),
            Some(code) => AttachmentAccessFailure::from_diagnostic_code(&code)
                .map(Err)
                .ok_or(()),
        })
        .collect()
}

pub(crate) fn run_stdio() -> ExitCode {
    match execute_stdio() {
        Ok(()) => ExitCode::SUCCESS,
        Err(()) => ExitCode::FAILURE,
    }
}

fn execute_stdio() -> Result<(), ()> {
    let mut input = Vec::new();
    std::io::stdin()
        .take(MAX_REQUEST_BYTES.saturating_add(1))
        .read_to_end(&mut input)
        .map_err(|_| ())?;
    if u64::try_from(input.len()).map_err(|_| ())? > MAX_REQUEST_BYTES {
        return Err(());
    }
    let request: WorkerRequest = serde_json::from_slice(&input).map_err(|_| ())?;
    if request.version != PROTOCOL_VERSION || request.paths.len() > MAX_BATCH_PATHS {
        return Err(());
    }
    let mut checker = FileAttachmentAccessibility;
    let failures = request
        .paths
        .iter()
        .map(|path| {
            checker
                .check(Path::new(path))
                .err()
                .map(|failure| failure.diagnostic_code().to_owned())
        })
        .collect();
    let output = serde_json::to_vec(&WorkerResponse {
        version: PROTOCOL_VERSION,
        failures,
    })
    .map_err(|_| ())?;
    std::io::stdout().write_all(&output).map_err(|_| ())
}

#[cfg(test)]
mod tests {
    use super::{decode_response, encode_request};

    #[test]
    fn protocol_round_trips_unicode_paths_and_rejects_wrong_counts() {
        let request =
            encode_request(vec!["/tmp/Grüße 第一.txt".to_owned()]).expect("request serialization");
        assert!(
            String::from_utf8(request)
                .expect("UTF-8 JSON")
                .contains("Grüße")
        );
        assert!(decode_response(br#"{"version":1,"failures":[null]}"#, 1).is_ok());
        assert!(decode_response(br#"{"version":1,"failures":[]}"#, 1).is_err());
    }
}
