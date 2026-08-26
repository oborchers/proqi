//! Shared parsing and bounded standard-input helpers.

use std::{io::Read as _, str::FromStr as _};

use sha2::Digest as _;

use crate::domain::{OperationId, RevisionId, ThoughtId};

use super::CliError;

pub(super) const MAX_THOUGHT_STDIN_BYTES: usize = 128 * 1024;
const HEX_DIGITS: &[u8; 16] = b"0123456789abcdef";

pub(super) fn parse_thought_id(value: &str) -> Result<ThoughtId, CliError> {
    ThoughtId::from_str(value).map_err(|error| {
        CliError::identifier(format!("invalid thought identifier {value}: {error}"))
    })
}

pub(super) fn parse_operation_id(value: Option<&str>) -> Result<Option<OperationId>, CliError> {
    value
        .map(|value| {
            OperationId::from_str(value).map_err(|error| {
                CliError::identifier(format!("invalid operation identifier {value}: {error}"))
            })
        })
        .transpose()
}

pub(super) fn parse_revision_id(value: Option<&str>) -> Result<Option<RevisionId>, CliError> {
    value
        .map(|value| {
            RevisionId::from_str(value).map_err(|error| {
                CliError::identifier(format!("invalid revision identifier {value}: {error}"))
            })
        })
        .transpose()
}

pub(super) fn read_standard_input() -> Result<String, CliError> {
    let mut content = String::new();
    std::io::stdin()
        .take((MAX_THOUGHT_STDIN_BYTES + 1) as u64)
        .read_to_string(&mut content)
        .map_err(|error| CliError::input(format!("read standard input: {error}")))?;
    if content.len() > MAX_THOUGHT_STDIN_BYTES {
        return Err(CliError::input(format!(
            "thought content exceeds the {MAX_THOUGHT_STDIN_BYTES}-byte standard-input limit"
        )));
    }
    Ok(content)
}

pub(super) fn content_digest_hex(content: &str) -> String {
    let digest = sha2::Sha256::digest(content.as_bytes());
    let mut encoded = String::with_capacity(digest.len() * 2);
    for byte in digest {
        encoded.push(char::from(HEX_DIGITS[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX_DIGITS[usize::from(byte & 0x0f)]));
    }
    encoded
}

pub(super) fn excerpt(content: &str) -> String {
    content
        .chars()
        .take(80)
        .collect::<String>()
        .replace('\n', " ")
}
