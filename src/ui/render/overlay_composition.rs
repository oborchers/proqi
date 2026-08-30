//! Overlay priority and view composition.

use ratatui_core::terminal::Frame;

use crate::ui::{BoardApp, LayoutSnapshot, Theme};

use super::{
    InvocationPickerView, PlainPickerView, overlays, render_invocation_picker, render_plain_picker,
};

pub(super) fn render(
    frame: &mut Frame<'_>,
    app: &BoardApp,
    layout: &LayoutSnapshot,
    theme: &Theme,
) {
    if render_decision(frame, app, layout, theme) {
        return;
    }
    if let Some((query, entries, selected)) = app.search_view() {
        if let Some(overlay) = &layout.overlay {
            render_plain_picker(
                frame,
                overlay,
                app,
                PlainPickerView {
                    title: " thoughts ",
                    prompt: '/',
                    query,
                    entries,
                    selected,
                },
                theme,
            );
        }
    } else if let Some((query, entries, selected)) = app.session_transfer_view() {
        if let Some(overlay) = &layout.overlay {
            render_plain_picker(
                frame,
                overlay,
                app,
                PlainPickerView {
                    title: " send to Proqi session ",
                    prompt: '/',
                    query,
                    entries,
                    selected,
                },
                theme,
            );
        }
    } else if let Some((query, entries, selected)) = app.discovered_invocation_view() {
        if let Some(overlay) = &layout.overlay {
            render_invocation_picker(
                frame,
                overlay,
                app,
                InvocationPickerView {
                    query,
                    entries,
                    selected,
                },
                theme,
            );
        }
    } else if let Some((query, entries, selected)) = app.palette_view() {
        if let Some(overlay) = &layout.overlay {
            render_plain_picker(
                frame,
                overlay,
                app,
                PlainPickerView {
                    title: " commands ",
                    prompt: ':',
                    query,
                    entries,
                    selected,
                },
                theme,
            );
        }
    } else if let Some(value) = app.session_rename_view() {
        if let Some(overlay) = &layout.overlay {
            overlays::render_text_prompt(frame, overlay, " rename session ", value, theme);
        }
    } else if app.help
        && let Some(overlay) = &layout.overlay
    {
        overlays::render_help(frame, app, overlay, theme);
    }
}

fn render_decision(
    frame: &mut Frame<'_>,
    app: &BoardApp,
    layout: &LayoutSnapshot,
    theme: &Theme,
) -> bool {
    let Some(overlay) = &layout.overlay else {
        return false;
    };
    if let Some((entries, selected)) = app.screenshot_takeover_view() {
        overlays::render_update(
            frame,
            overlay,
            " screenshot inbox in use ",
            &entries,
            selected,
            theme,
        );
        true
    } else if let Some((title, entries, selected)) = app.update_prompt_view() {
        overlays::render_update(frame, overlay, &title, &entries, selected, theme);
        true
    } else {
        false
    }
}
