//! Bounded best-effort discovery of local authoring definitions.

mod metadata;
mod plugins;
mod roots;
mod scan;

pub use scan::FilesystemInvocationCatalog;

#[cfg(test)]
mod tests;
