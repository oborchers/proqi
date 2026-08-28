//! Adapters for persistence, terminals, clipboards, integrations, and runtime coordination.

pub mod attachment;
pub mod clipboard;
pub mod control;
pub mod diagnostics;
pub mod doctor;
pub mod editor;
pub(crate) mod filesystem;
pub mod herdr;
pub mod invocation;
pub mod memory;
pub mod process;
pub mod recovery;
pub mod runtime;
pub mod sqlite;
pub mod terminal;
pub mod update;
