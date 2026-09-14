//! Unified terminal-independent shortcut registry and dispatcher.

mod command_execution;
mod config;
mod context_policy;
mod contract;
mod dispatch;
mod errors;
mod inspection;
mod intentions;
mod inventory;
pub(crate) mod legacy;
mod legacy_validation;
mod model;
pub(crate) mod presentation;
mod validation;

pub(crate) use command_execution::{
    BoardCommand as PaletteBoardCommand, CommandExecution, EditorCommand as PaletteEditorCommand,
    EntryCommand as PaletteEntryCommand, PasteCommand as PalettePasteCommand,
    RuntimeCommand as PaletteRuntimeCommand, SelectionCommand as PaletteSelectionCommand,
    SubmissionCommand as PaletteSubmissionCommand,
    TransformationCommand as PaletteTransformationCommand,
};
pub(crate) use config::KeymapDocument;
#[cfg(test)]
pub(crate) use dispatch::ResolvedShortcut;
pub(crate) use dispatch::ShortcutPlatform;
pub use dispatch::ShortcutRegistry;
pub use errors::ShortcutRegistryError;
pub(crate) use inspection::{ShortcutInspection, ShortcutStrokeInspection};
pub(super) use inventory::fixed_character_binding;
#[cfg(test)]
pub(crate) use model::CommandCategory;
pub(crate) use model::{
    CommandApplicability, CommandDiscoverability, CommandLabel, CommandMetadata, CommandRelevance,
    CommandScope, HelpAvailability, HelpSurface,
};
pub use model::{
    ShortcutActionId, ShortcutBinding, ShortcutBindingClaim, ShortcutContext, ShortcutContextStack,
    ShortcutDescriptor, ShortcutModifiers, ShortcutSafety,
};

#[cfg(test)]
mod tests;
