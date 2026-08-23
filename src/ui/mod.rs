//! Responsive layout, rendering, keymaps, and hit testing.

mod app;
mod browser;
mod browser_render;
mod layout;
mod render;
mod settings;
mod theme;

pub use app::{BoardApp, PointerButton, PointerInput, PointerKind, UiInput, UiKey};
pub use browser::{
    BrowserAction, BrowserAvailability, BrowserEntryLayout, BrowserLayout, RecencyGroup,
    SessionBrowser, SessionBrowserItem,
};
pub use browser_render::render_browser;
pub use layout::{HitTarget, LayoutSnapshot, ThoughtLayout, compute as compute_layout};
pub use render::render;
pub use settings::{KeyBindings, ThemePreference, UiSettings};
pub use theme::Theme;
