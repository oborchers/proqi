//! Harness compatibility policy and session identity validation.

use crate::ports::agent::{
    AgentError, AgentSessionBinding, AgentTarget, CODEX_AGENT_KIND, HarnessKind,
};

use super::contract::AgentSession;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SessionPolicy {
    Required,
    ProvisionalAllowed,
}

pub(super) fn kind(value: String) -> Result<HarnessKind, AgentError> {
    HarnessKind::new(value)
        .ok_or_else(|| AgentError::Unsupported("neighbor is not a recognized agent".to_owned()))
}

pub(super) fn discovered_session(
    kind: &HarnessKind,
    session: Option<&AgentSession>,
) -> Result<AgentSessionBinding, AgentError> {
    let Some(session) = session else {
        return match session_policy(kind) {
            SessionPolicy::ProvisionalAllowed => Ok(AgentSessionBinding::provisional()),
            SessionPolicy::Required => Err(AgentError::Unsupported(
                "neighbor has no agent session identity".to_owned(),
            )),
        };
    };
    established_session(kind, session)
}

pub(super) fn receipt_session(
    target: &AgentTarget,
    session: Option<&AgentSession>,
) -> Result<AgentSessionBinding, AgentError> {
    let Some(session) = session else {
        return if target.agent_session.is_provisional() {
            Ok(AgentSessionBinding::provisional())
        } else {
            Err(AgentError::Malformed(
                "prompt receipt lost its agent session identity".to_owned(),
            ))
        };
    };
    let binding = established_session(&target.agent_kind, session)?;
    if target.agent_session.accepts_receipt(&binding) {
        Ok(binding)
    } else {
        Err(AgentError::Malformed(
            "prompt receipt has an inconsistent agent session identity".to_owned(),
        ))
    }
}

fn established_session(
    kind: &HarnessKind,
    session: &AgentSession,
) -> Result<AgentSessionBinding, AgentError> {
    if session.agent != kind.as_str()
        || session.kind.trim().is_empty()
        || session.source.trim().is_empty()
    {
        return Err(AgentError::Malformed(
            "agent session identity is inconsistent".to_owned(),
        ));
    }
    AgentSessionBinding::established(session.value.clone())
        .ok_or_else(|| AgentError::Malformed("agent session identity is inconsistent".to_owned()))
}

fn session_policy(kind: &HarnessKind) -> SessionPolicy {
    match kind.as_str() {
        CODEX_AGENT_KIND => SessionPolicy::ProvisionalAllowed,
        _ => SessionPolicy::Required,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_explicitly_compatible_harnesses_allow_provisional_sessions() {
        for (name, expected) in [
            (CODEX_AGENT_KIND, SessionPolicy::ProvisionalAllowed),
            ("claude", SessionPolicy::Required),
            ("future-harness", SessionPolicy::Required),
        ] {
            let kind = HarnessKind::new(name).expect("valid fixture harness");
            assert_eq!(session_policy(&kind), expected, "{name}");
        }
    }
}
