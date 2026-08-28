//! Structured invocation picker projections.

/// One invocation result rendered as a responsive two-field row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::ui) struct InvocationChoiceView {
    /// Exact canonical token inserted when this row is accepted.
    pub(in crate::ui) token: String,
    /// Scope, kind, and collision-specific harness provenance.
    pub(in crate::ui) qualifier: String,
}
