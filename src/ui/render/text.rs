//! Semantic styling of canonical text into terminal cells.

use linkify::{LinkFinder, LinkKind};
use ratatui_core::{
    style::{Modifier, Style},
    text::{Line, Span},
};
use unicode_segmentation::UnicodeSegmentation;

use crate::ui::Theme;

pub(super) fn styled_line(
    content: &str,
    line: &crate::ports::editor::VisualLine,
    semantic_styles: &[crate::ui::annotations::PresentedStyle],
    links: &[std::ops::Range<usize>],
    invocations: &[std::ops::Range<usize>],
    theme: &Theme,
) -> Line<'static> {
    let source = content
        .get(line.start_byte..line.end_byte)
        .unwrap_or_default();
    let mut column = 0;
    let spans = source
        .grapheme_indices(true)
        .map(|(offset, grapheme)| {
            let byte = line.start_byte.saturating_add(offset);
            let (visible, width) = crate::ports::text_layout::display_grapheme(grapheme, column);
            let selected = line.selected_cells.is_some_and(|selection| {
                column < selection.end && column.saturating_add(width) > selection.start
            });
            let semantic = semantic_styles
                .iter()
                .find(|style| byte >= style.start && byte < style.end)
                .map(|style| style.kind);
            let linked = links.iter().any(|range| range.contains(&byte));
            let invocation = invocations.iter().any(|range| range.contains(&byte));
            column = column.saturating_add(width);
            let mut style = Style::default().fg(
                if matches!(
                    semantic,
                    Some(crate::ui::annotations::PresentedStyleKind::Warning)
                ) {
                    theme.warning
                } else if semantic.is_some() || invocation {
                    theme.annotation
                } else if linked {
                    theme.link
                } else {
                    theme.foreground
                },
            );
            if semantic.is_some() || invocation {
                style = style.add_modifier(Modifier::BOLD);
            }
            if linked {
                style = style.add_modifier(Modifier::UNDERLINED);
            }
            if selected {
                style = style.add_modifier(Modifier::REVERSED);
            }
            Span::styled(visible, style)
        })
        .collect::<Vec<_>>();
    Line::from(spans)
}

pub(super) fn url_ranges(content: &str) -> Vec<std::ops::Range<usize>> {
    let mut finder = LinkFinder::new();
    finder.kinds(&[LinkKind::Url]);
    finder
        .links(content)
        .filter(|link| {
            let value = link.as_str();
            value
                .get(..7)
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case("http://"))
                || value
                    .get(..8)
                    .is_some_and(|prefix| prefix.eq_ignore_ascii_case("https://"))
        })
        .map(|link| link.start()..link.end())
        .collect()
}
