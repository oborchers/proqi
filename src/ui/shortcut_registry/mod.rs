//! Unified terminal-independent shortcut registry and dispatcher.

mod context_policy;
mod dispatch;
mod intentions;
mod inventory;
mod model;
pub(crate) mod presentation;
mod validation;

pub(crate) use dispatch::ShortcutRegistry;
#[cfg(test)]
pub(crate) use dispatch::{ResolvedShortcut, ShortcutPlatform};
pub(crate) use model::{
    CommandAvailability, CommandLabel, CommandMetadata, HelpAvailability, HelpSurface,
};
pub use model::{
    ShortcutActionId, ShortcutBinding, ShortcutBindingClaim, ShortcutContext, ShortcutContextStack,
    ShortcutDescriptor, ShortcutIntention, ShortcutModifiers, ShortcutSafety,
};

#[cfg(test)]
mod tests;
