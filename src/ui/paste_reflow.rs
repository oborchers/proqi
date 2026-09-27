//! UI compatibility facade over the application-owned spacing-cleanup policy.

pub(in crate::ui) use crate::application::text_reflow::position_changes;
#[cfg(test)]
pub(in crate::ui) use crate::application::text_reflow::reflow_text_isolated;
