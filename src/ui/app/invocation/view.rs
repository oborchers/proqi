//! Structured invocation picker projections.

#[derive(Clone)]
pub(super) struct Choice {
    pub(super) token: String,
    pub(super) insertion: String,
    pub(super) separate_from_prefix: bool,
    pub(super) qualifier: String,
    pub(super) group: Option<String>,
}

/// One invocation result rendered as a responsive two-field row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::ui) struct InvocationChoiceView {
    /// Bounded primary text shown for the result.
    pub(in crate::ui) token: String,
    /// Scope, kind, and collision-specific harness provenance.
    pub(in crate::ui) qualifier: String,
    /// Live topology group rendered immediately above this choice.
    pub(in crate::ui) group: Option<String>,
}
