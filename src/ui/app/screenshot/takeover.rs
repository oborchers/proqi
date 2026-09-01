//! Verified Screenshot Inbox ownership takeover interaction.

use crate::{
    application::{Effect, ScreenshotIntent},
    ports::environment::{Clock, IdGenerator},
    ui::{ListNavigation, UiInput, UiKey},
};

use super::super::BoardApp;
use super::ScreenshotState;

impl BoardApp {
    pub(in crate::ui::app) fn handle_screenshot_takeover_input(
        &mut self,
        input: &UiInput,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        match input {
            UiInput::Key(UiKey::Escape) => self.cancel_screenshot_takeover(),
            UiInput::Key(UiKey::Enter) => return self.choose_screenshot_takeover(ids),
            UiInput::Key(key) if key.list_navigation() == Some(ListNavigation::Previous) => {
                self.screenshot.takeover_selected = 0;
            }
            UiInput::Key(key) if key.list_navigation() == Some(ListNavigation::Next) => {
                self.screenshot.takeover_selected = 1;
            }
            UiInput::Pointer(pointer) => return self.handle_pointer(*pointer, ids, clock),
            UiInput::Resize { .. }
            | UiInput::HostFocusGained
            | UiInput::HostFocusLost
            | UiInput::Paste(_)
            | UiInput::PasteAnnotated(_)
            | UiInput::Key(_) => {}
        }
        Vec::new()
    }

    pub(in crate::ui::app) fn choose_screenshot_takeover(
        &mut self,
        ids: &mut impl IdGenerator,
    ) -> Vec<Effect> {
        if self.screenshot.takeover_selected == 0 {
            self.cancel_screenshot_takeover();
            return Vec::new();
        }
        let Some(owner) = self.screenshot.takeover.take() else {
            return Vec::new();
        };
        self.screenshot.state = ScreenshotState::Starting;
        vec![Effect::Screenshot(ScreenshotIntent::TakeOver {
            owner,
            request_id: ids.request_id(),
        })]
    }

    pub(in crate::ui::app) fn cancel_screenshot_takeover(&mut self) {
        self.screenshot.takeover = None;
        self.screenshot.takeover_selected = 0;
    }

    pub(in crate::ui) fn screenshot_takeover_view(&self) -> Option<(Vec<String>, usize)> {
        self.screenshot.takeover.as_ref().map(|_| {
            (
                vec!["Cancel".to_owned(), "Take over".to_owned()],
                self.screenshot.takeover_selected,
            )
        })
    }
}
