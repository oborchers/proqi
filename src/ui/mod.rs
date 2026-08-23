//! Responsive layout, rendering, keymaps, and hit testing.

mod app;
mod layout;
mod render;
mod settings;
mod theme;

pub use app::{BoardApp, PointerButton, PointerInput, PointerKind, UiInput, UiKey};
pub use layout::{HitTarget, LayoutSnapshot, ThoughtLayout, compute as compute_layout};
pub use render::render;
pub use settings::{KeyBindings, ThemePreference, UiSettings};
pub use theme::Theme;
