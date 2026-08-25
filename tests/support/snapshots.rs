use ratatui_core::buffer::Buffer;

pub(crate) fn snapshot_buffer(buffer: &Buffer) -> String {
    format!(
        "SIZE {}x{}\n\nTEXT\n{}\n\nSTYLE RUNS\n{}",
        buffer.area.width,
        buffer.area.height,
        snapshot_text(buffer),
        style_runs(buffer)
    )
}

fn snapshot_text(buffer: &Buffer) -> String {
    (0..buffer.area.height)
        .map(|y| {
            let row = (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>();
            format!("{y:02}│{}│", row.trim_end_matches(' '))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn style_runs(buffer: &Buffer) -> String {
    let mut rows = Vec::new();
    for y in 0..buffer.area.height {
        let mut runs = Vec::new();
        let mut start = 0;
        let mut previous = style_key(buffer, 0, y);
        for x in 1..buffer.area.width {
            let current = style_key(buffer, x, y);
            if current != previous {
                runs.push(format!("{start}-{end} {previous}", end = x - 1));
                start = x;
                previous = current;
            }
        }
        runs.push(format!(
            "{start}-{end} {previous}",
            end = buffer.area.width.saturating_sub(1)
        ));
        rows.push(format!("{y}: {}", runs.join(" | ")));
    }
    rows.join("\n")
}

fn style_key(buffer: &Buffer, x: u16, y: u16) -> String {
    let cell = &buffer[(x, y)];
    format!("fg={:?} bg={:?} mod={:?}", cell.fg, cell.bg, cell.modifier)
}
