//! Responsive picker row data and rendering.

use ratatui_core::{
    style::Modifier,
    text::{Line, Span},
};

use super::{Theme, cell_width, ellipsize};

#[derive(Clone, Copy)]
pub(in crate::ui::render) struct PickerView<'a> {
    pub(in crate::ui::render) title: &'a str,
    pub(in crate::ui::render) prompt: char,
    pub(in crate::ui::render) query: &'a str,
    pub(in crate::ui::render) cursor: usize,
    pub(in crate::ui::render) entries: &'a [PickerRow<'a>],
    pub(in crate::ui::render) selected: usize,
}

#[derive(Clone, Copy)]
pub(in crate::ui::render) struct PickerRow<'a> {
    pub(super) primary: &'a str,
    pub(super) secondary: Option<&'a str>,
    pub(super) secondary_fallbacks: &'a [String],
    pub(super) protected_secondaries: &'a [String],
    pub(super) group: Option<&'a str>,
    pub(super) enabled: bool,
}

impl<'a> PickerRow<'a> {
    pub(in crate::ui::render) const fn plain(primary: &'a str) -> Self {
        Self {
            primary,
            secondary: None,
            secondary_fallbacks: &[],
            protected_secondaries: &[],
            group: None,
            enabled: true,
        }
    }

    #[cfg(test)]
    pub(super) const fn fields(primary: &'a str, secondary: &'a str) -> Self {
        Self {
            primary,
            secondary: Some(secondary),
            secondary_fallbacks: &[],
            protected_secondaries: &[],
            group: None,
            enabled: true,
        }
    }

    pub(in crate::ui::render) const fn grouped(
        primary: &'a str,
        secondary: &'a str,
        secondary_fallbacks: &'a [String],
        group: Option<&'a str>,
    ) -> Self {
        Self {
            primary,
            secondary: Some(secondary),
            secondary_fallbacks,
            protected_secondaries: &[],
            group,
            enabled: true,
        }
    }

    pub(in crate::ui::render) const fn choice(
        primary: &'a str,
        secondary: &'a str,
        enabled: bool,
    ) -> Self {
        Self {
            primary,
            secondary: Some(secondary),
            secondary_fallbacks: &[],
            protected_secondaries: &[],
            group: None,
            enabled,
        }
    }

    pub(in crate::ui::render) const fn responsive_choice(
        primary: &'a str,
        secondary: &'a str,
        secondary_fallbacks: &'a [String],
        protected_secondaries: &'a [String],
        enabled: bool,
    ) -> Self {
        Self {
            primary,
            secondary: Some(secondary),
            secondary_fallbacks,
            protected_secondaries,
            group: None,
            enabled,
        }
    }

    pub(in crate::ui::render) fn command(
        primary: &'a str,
        secondary: Option<&'a str>,
        secondary_fallbacks: &'a [String],
        protected_secondaries: &'a [String],
        group: Option<&'a str>,
        enabled: bool,
    ) -> Self {
        Self {
            primary,
            secondary,
            secondary_fallbacks,
            protected_secondaries,
            group,
            enabled,
        }
    }
}

#[cfg(test)]
pub(super) fn picker_row(entry: PickerRow<'_>, width: u16) -> String {
    let (primary, secondary) = picker_content(entry, usize::from(width));
    let Some(secondary) = secondary else {
        return primary;
    };
    let gap = usize::from(width).saturating_sub(cell_width(&primary) + cell_width(&secondary));
    format!("{primary}{}{secondary}", " ".repeat(gap))
}

pub(super) fn picker_line(
    entry: PickerRow<'_>,
    width: u16,
    selected: bool,
    theme: &Theme,
) -> Line<'static> {
    if entry.secondary.is_none() {
        let base = if selected {
            theme.focused_style()
        } else {
            theme.base_style()
        };
        let style = if !entry.enabled {
            base.fg(theme.muted)
        } else if selected {
            base.fg(theme.accent).add_modifier(Modifier::BOLD)
        } else {
            base
        };
        let mut content = ellipsize(entry.primary, usize::from(width));
        content.push_str(&" ".repeat(usize::from(width).saturating_sub(cell_width(&content))));
        return Line::from(Span::styled(content, style));
    }
    let width = usize::from(width);
    let (primary, secondary) = picker_content(entry, width);
    let base = if selected {
        theme.focused_style()
    } else {
        theme.base_style()
    };
    let primary_style = if !entry.enabled {
        base.fg(theme.muted)
    } else if selected {
        base.fg(theme.accent).add_modifier(Modifier::BOLD)
    } else {
        base.fg(theme.foreground)
    };
    let Some(secondary) = secondary else {
        let padding = " ".repeat(width.saturating_sub(cell_width(&primary)));
        return Line::from(vec![
            Span::styled(primary, primary_style),
            Span::styled(padding, base),
        ]);
    };
    let gap = width.saturating_sub(cell_width(&primary) + cell_width(&secondary));
    Line::from(vec![
        Span::styled(primary, primary_style),
        Span::styled(" ".repeat(gap), base),
        Span::styled(secondary, base.fg(theme.muted)),
    ])
}

fn fitting_secondary(entry: PickerRow<'_>, width: usize) -> Option<&str> {
    let minimum_gap = usize::from(entry.group.is_none()) + 1;
    entry
        .secondary
        .into_iter()
        .chain(entry.secondary_fallbacks.iter().map(String::as_str))
        .chain(entry.protected_secondaries.iter().map(String::as_str))
        .find(|secondary| {
            cell_width(entry.primary)
                .saturating_add(minimum_gap)
                .saturating_add(cell_width(secondary))
                <= width
        })
}

fn picker_content(entry: PickerRow<'_>, width: usize) -> (String, Option<String>) {
    let primary = display(entry.primary);
    if let Some(secondary) = fitting_secondary(entry, width) {
        return (primary, Some(display(secondary)));
    }
    let minimum_gap = usize::from(entry.group.is_none()) + 1;
    for secondary in entry.protected_secondaries {
        let secondary_width = cell_width(secondary);
        if minimum_gap
            .saturating_add(1)
            .saturating_add(secondary_width)
            <= width
        {
            let primary_width = width.saturating_sub(minimum_gap + secondary_width);
            return (
                ellipsize(entry.primary, primary_width),
                Some(display(secondary)),
            );
        }
        if secondary_width <= width {
            return (String::new(), Some(display(secondary)));
        }
    }
    if let Some(secondary) = entry.protected_secondaries.last() {
        return (String::new(), Some(ellipsize(secondary, width)));
    }
    (ellipsize(entry.primary, width), None)
}

fn display(value: &str) -> String {
    crate::ports::text_layout::truncate_cells(value, cell_width(value))
}
