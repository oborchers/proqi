//! Exact CLI edits shared by inactive sessions and active owners.

use crate::{adapters::runtime::SystemIdGenerator, ports::environment::IdGenerator as _};

use super::helpers::{parse_operation_id, parse_thought_id, read_standard_input};
use super::{Outcome, forwarding, mutation_outcome};
use crate::cli::{output::CliError, runtime::RuntimeContext};

pub(super) fn replace(
    context: &mut RuntimeContext,
    session: &str,
    thought: &str,
    expected: Option<&str>,
    force: bool,
) -> Result<Outcome, CliError> {
    let thought_id = parse_thought_id(thought)?;
    let expected_digest = if force {
        None
    } else {
        Some(parse_digest(expected.ok_or_else(|| {
            CliError::arguments("replacement requires --expected-sha256 or --force".to_owned())
        })?)?)
    };
    let replacement = read_standard_input()?;
    let revision_id = context.ids.revision_id();
    let mut service = super::session_service(context)?;
    let session_id = service.resolve_session(session, false)?;
    drop(service);
    if let Some(result) = forwarding::replace(
        context,
        session_id,
        thought_id,
        replacement.clone(),
        expected_digest,
        revision_id,
    )? {
        return Ok(mutation_outcome(result.thought_id, result.receipt));
    }
    let result = super::session_service(context)?.replace_thought(
        session_id,
        thought_id,
        replacement,
        expected_digest,
        revision_id,
    )?;
    Ok(mutation_outcome(result.thought_id, result.receipt))
}

pub(super) fn collapse(
    context: &mut RuntimeContext,
    session: &str,
    thought: &str,
    collapsed: bool,
    operation: Option<&str>,
) -> Result<Outcome, CliError> {
    let thought_id = parse_thought_id(thought)?;
    let supplied = parse_operation_id(operation)?;
    let mut service = super::session_service(context)?;
    let session_id = service.resolve_session(session, false)?;
    drop(service);
    if let Some(result) =
        forwarding::set_collapsed(context, session_id, thought_id, collapsed, supplied)?
    {
        return Ok(mutation_outcome(result.thought_id, result.receipt));
    }
    let operation_id = supplied.unwrap_or_else(|| next_operation_id(&mut context.ids));
    let result = super::session_service(context)?.set_thought_collapsed(
        session_id,
        thought_id,
        collapsed,
        operation_id,
    )?;
    Ok(mutation_outcome(result.thought_id, result.receipt))
}

fn next_operation_id(ids: &mut SystemIdGenerator) -> crate::domain::OperationId {
    ids.operation_id()
}

fn parse_digest(value: &str) -> Result<[u8; 32], CliError> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(CliError::arguments(
            "SHA-256 precondition must be exactly 64 hexadecimal characters".to_owned(),
        ));
    }
    let mut digest = [0; 32];
    for (index, byte) in digest.iter_mut().enumerate() {
        let offset = index * 2;
        *byte = u8::from_str_radix(&value[offset..offset + 2], 16)
            .map_err(|error| CliError::arguments(error.to_string()))?;
    }
    Ok(digest)
}

#[cfg(test)]
mod tests {
    use super::parse_digest;

    #[test]
    fn digest_parser_requires_complete_sha256_hex() {
        assert_eq!(parse_digest(&"ab".repeat(32)).expect("digest"), [0xab; 32]);
        assert!(parse_digest("ab").is_err());
        assert!(parse_digest(&"zz".repeat(32)).is_err());
    }
}
