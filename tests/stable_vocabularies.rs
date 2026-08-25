//! Stable spelling contracts for durable and externally visible vocabularies.

use proqi::{
    domain::Direction,
    ports::{
        agent::{AgentError, AgentFailureCode, AgentState, SubmissionDisposition},
        control::ControlRejectionCode,
        store::SubmissionAttemptState,
    },
};

#[test]
fn durable_and_external_vocabularies_have_stable_spellings() {
    assert_eq!(Direction::Left.as_str(), "left");
    assert_eq!(AgentState::Working.as_str(), "working");
    assert_eq!(
        SubmissionDisposition::RemoveAfterSuccess.as_str(),
        "remove_after_success"
    );
    assert_eq!(
        SubmissionAttemptState::OutcomeUnknown.as_str(),
        "outcome_unknown"
    );
    assert_eq!(
        ControlRejectionCode::RequestIdConflict.as_str(),
        "request_id_conflict"
    );
}

#[test]
fn agent_errors_share_one_stable_classification() {
    let cases = [
        (
            AgentError::Unavailable(String::new()),
            AgentFailureCode::Unavailable,
        ),
        (
            AgentError::Unsupported(String::new()),
            AgentFailureCode::Unsupported,
        ),
        (
            AgentError::Malformed(String::new()),
            AgentFailureCode::Malformed,
        ),
        (
            AgentError::Ambiguous(String::new()),
            AgentFailureCode::Ambiguous,
        ),
        (AgentError::TimedOut, AgentFailureCode::TimedOut),
        (
            AgentError::Rejected {
                code: String::new(),
                message: String::new(),
            },
            AgentFailureCode::Rejected,
        ),
        (
            AgentError::Process(String::new()),
            AgentFailureCode::ProcessFailed,
        ),
    ];
    for (error, expected) in cases {
        assert_eq!(error.stable_code(), expected);
        assert!(!expected.as_str().is_empty());
    }
}
