//! Responsive layout, rendering, keymaps, and hit testing.

mod annotations;
mod app;
mod browser;
mod browser_render;
mod browser_summary;
mod layout;
mod projection;
mod render;
mod settings;
mod status;
mod theme;

pub use annotations::PastePayload;
pub use app::{BoardApp, PointerButton, PointerInput, PointerKind, UiInput, UiKey};
pub use browser::{
    BrowserAction, BrowserAvailability, BrowserEntryLayout, BrowserLayout, RecencyGroup,
    SessionBrowser, SessionBrowserItem,
};
pub use browser_render::render_browser;
pub use layout::{HitTarget, LayoutSnapshot, ThoughtLayout, compute as compute_layout};
pub use render::render;
pub use settings::{BoardDensity, KeyBindings, KeyboardEnhancement, UiSettings};
pub use theme::{TerminalPalette, Theme, ThemePreference};
pub(crate) use theme::{ThemeOverrides, ThemeRecipe};
