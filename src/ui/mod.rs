//! Responsive layout, rendering, keymaps, and hit testing.

mod annotations;
mod app;
mod browser;
mod browser_render;
mod browser_summary;
mod control_labels;
mod geometry;
mod input;
mod layout;
mod paging;
mod projection;
mod render;
mod settings;
mod shortcut_metadata;
mod shortcuts;
mod status;
mod theme;

pub use annotations::PastePayload;
pub use app::BoardApp;
pub use browser::{
    BrowserAction, BrowserAvailability, BrowserEntryLayout, BrowserLayout, RecencyGroup,
    SessionBrowser, SessionBrowserItem,
};
pub use browser_render::render_browser;
pub(crate) use input::ListNavigation;
pub use input::{PointerButton, PointerInput, PointerKind, UiInput, UiKey, VisualRowEdge};
pub use layout::{HitTarget, LayoutSnapshot, ThoughtLayout, compute as compute_layout};
pub use paging::FastNavigation;
pub use render::render;
pub(crate) use render::render_with_outcome;
pub use settings::{BoardDensity, KeyBindings, KeyboardEnhancement, UiSettings};
pub use theme::{TerminalPalette, Theme, ThemePreference};
pub(crate) use theme::{ThemeOverrides, ThemeRecipe};
