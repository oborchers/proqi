//! Structured invocation picker projections.

#[derive(Clone)]
pub(super) struct Choice {
    pub(super) token: String,
    pub(super) insertion: String,
    pub(super) annotation_display: Option<String>,
    pub(super) separate_from_prefix: bool,
    pub(super) qualifier: String,
    pub(super) qualifier_fallbacks: Vec<String>,
    pub(super) group: Option<String>,
}

/// One invocation result rendered as a responsive two-field row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::ui) struct InvocationChoiceView {
    /// Bounded primary text shown for the result.
    pub(in crate::ui) token: String,
    /// Scope or compact live topology and harness context.
    pub(in crate::ui) qualifier: String,
    /// Progressively quieter descriptions used when the full qualifier does not fit.
    pub(in crate::ui) qualifier_fallbacks: Vec<String>,
    /// Structural group rendered immediately above the first visible matching choice.
    pub(in crate::ui) group: Option<String>,
}
