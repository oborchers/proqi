use ratatui_core::{buffer::Buffer, layout::Rect, text::Line, widgets::Widget};
use ratatui_widgets::paragraph::Paragraph;

use crate::ports::text_layout::wrap_rows;

#[test]
fn multiline_thoughts_are_distinct_buffer_rows() {
    let area = Rect::new(0, 0, 12, 2);
    let mut buffer = Buffer::empty(area);
    let lines = wrap_rows("first\n第二", 12)
        .into_iter()
        .map(|row| Line::raw(row.visual.text))
        .collect::<Vec<_>>();
    Paragraph::new(lines).render(area, &mut buffer);
    assert_eq!(buffer[(0, 0)].symbol(), "f");
    assert_eq!(buffer[(0, 1)].symbol(), "第");
}
