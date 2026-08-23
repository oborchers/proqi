//! Responsive session-browser layout and authoritative hit geometry.

use ratatui_core::layout::Rect;

use super::{BrowserEntryLayout, BrowserHit, BrowserLayout, SessionBrowser};

impl BrowserLayout {
    pub(super) fn hit_test(&self, column: u16, row: u16) -> BrowserHit {
        if contains(self.footer, column, row) {
            let offset = column.saturating_sub(self.footer.x);
            return if offset < 11 {
                BrowserHit::Rename
            } else if offset < 22 {
                BrowserHit::Trash
            } else {
                BrowserHit::Cancel
            };
        }
        self.entries
            .iter()
            .find(|entry| {
                contains(entry.row, column, row)
                    || entry
                        .inline_detail
                        .is_some_and(|area| contains(area, column, row))
            })
            .map_or(BrowserHit::None, |entry| BrowserHit::Item(entry.item_index))
    }
}

impl SessionBrowser {
    pub(super) fn compute_layout(&self, area: Rect) -> BrowserLayout {
        let header_height = area.height.min(2);
        let footer_height = u16::from(area.height > header_height);
        let header = Rect::new(area.x, area.y, area.width, header_height);
        let body_y = area.y.saturating_add(header_height);
        let body_height = area.height.saturating_sub(header_height + footer_height);
        let footer = Rect::new(
            area.x,
            area.bottom().saturating_sub(footer_height),
            area.width,
            footer_height,
        );
        let wide = area.width >= 72;
        let result_width = if wide {
            area.width.saturating_mul(3) / 5
        } else {
            area.width
        };
        let results = Rect::new(area.x, body_y, result_width, body_height);
        let detail = wide.then(|| {
            Rect::new(
                results.right(),
                body_y,
                area.width.saturating_sub(result_width),
                body_height,
            )
        });
        BrowserLayout {
            area,
            header,
            results,
            detail,
            entries: self.place_entries(results, !wide),
            footer,
        }
    }

    fn place_entries(&self, area: Rect, inline: bool) -> Vec<BrowserEntryLayout> {
        let mut entries = Vec::new();
        let mut y = area.y;
        let mut previous_group = None;
        for (filtered_position, item_index) in self
            .filtered
            .iter()
            .copied()
            .enumerate()
            .skip(self.first_visible)
        {
            let item = &self.items[item_index];
            let group = self.group_for(item);
            let group_area =
                (previous_group != Some(group)).then(|| Rect::new(area.x, y, area.width, 1));
            if group_area.is_some() {
                y = y.saturating_add(1);
            }
            if y >= area.bottom() {
                break;
            }
            let row_height = area.bottom().saturating_sub(y).min(2);
            let row = Rect::new(area.x, y, area.width, row_height);
            y = y.saturating_add(row_height);
            let inline_detail = (inline && filtered_position == self.selected && y < area.bottom())
                .then(|| inline_detail(area, &mut y));
            entries.push(BrowserEntryLayout {
                item_index,
                group: group_area.map(|area| (group, area)),
                row,
                inline_detail,
            });
            previous_group = Some(group);
            if y >= area.bottom() {
                break;
            }
        }
        entries
    }
}

fn inline_detail(area: Rect, y: &mut u16) -> Rect {
    let height = area.bottom().saturating_sub(*y).min(8);
    let detail = Rect::new(
        area.x.saturating_add(2),
        *y,
        area.width.saturating_sub(2),
        height,
    );
    *y = y.saturating_add(height);
    detail
}

fn contains(area: Rect, column: u16, row: u16) -> bool {
    column >= area.x && column < area.right() && row >= area.y && row < area.bottom()
}
