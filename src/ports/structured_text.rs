//! Shared recognition for Markdown structure without rendering or editor policy.

#[derive(Clone, Copy)]
pub(crate) struct ListMarker<'a> {
    indentation: &'a str,
    marker: Marker<'a>,
    spacing: &'a str,
    task_spacing: Option<&'a str>,
    content: &'a str,
}

#[derive(Clone, Copy)]
enum Marker<'a> {
    Bullet(char),
    Ordered { number: &'a str, delimiter: char },
}

impl ListMarker<'_> {
    pub(crate) const fn indentation(&self) -> &str {
        self.indentation
    }

    pub(crate) const fn content(&self) -> &str {
        self.content
    }

    pub(crate) fn prefix_len(&self) -> usize {
        self.indentation.len()
            + match self.marker {
                Marker::Bullet(marker) => marker.len_utf8(),
                Marker::Ordered { number, delimiter } => number.len() + delimiter.len_utf8(),
            }
            + self.spacing.len()
            + self.task_spacing.map_or(0, |spacing| 3 + spacing.len())
    }

    pub(crate) fn content_column(&self) -> usize {
        indentation_columns(&self.indentation_and_prefix())
    }

    pub(crate) fn indentation_columns(&self) -> usize {
        indentation_columns(self.indentation)
    }

    pub(crate) fn continuation(&self) -> String {
        let marker = match self.marker {
            Marker::Bullet(bullet) => bullet.to_string(),
            Marker::Ordered { number, delimiter } => {
                number
                    .parse::<u64>()
                    .map_or_else(|_| number.to_owned(), |value| (value + 1).to_string())
                    + &delimiter.to_string()
            }
        };
        let task = self
            .task_spacing
            .map_or_else(String::new, |spacing| format!("[ ]{spacing}"));
        format!("{}{marker}{}{task}", self.indentation, self.spacing)
    }

    fn indentation_and_prefix(&self) -> String {
        let mut prefix = String::with_capacity(self.prefix_len());
        prefix.push_str(self.indentation);
        match self.marker {
            Marker::Bullet(marker) => prefix.push(marker),
            Marker::Ordered { number, delimiter } => {
                prefix.push_str(number);
                prefix.push(delimiter);
            }
        }
        prefix.push_str(self.spacing);
        if let Some(spacing) = self.task_spacing {
            prefix.push_str("[ ]");
            prefix.push_str(spacing);
        }
        prefix
    }
}

pub(crate) fn parse_list_marker(line: &str) -> Option<ListMarker<'_>> {
    let indentation_end = whitespace_prefix(line);
    let indentation = &line[..indentation_end];
    let rest = &line[indentation_end..];
    let (marker, marker_end) = parse_base_marker(rest)?;
    let spacing_end = marker_end + whitespace_prefix(&rest[marker_end..]);
    if spacing_end == marker_end {
        return None;
    }
    let spacing = &rest[marker_end..spacing_end];
    let after_marker = &rest[spacing_end..];
    let (task_spacing, content) =
        parse_task(after_marker).map_or((None, after_marker), |parsed| (Some(parsed.0), parsed.1));
    Some(ListMarker {
        indentation,
        marker,
        spacing,
        task_spacing,
        content,
    })
}

fn parse_base_marker(rest: &str) -> Option<(Marker<'_>, usize)> {
    let first = rest.chars().next()?;
    if matches!(first, '-' | '*' | '+') {
        return Some((Marker::Bullet(first), first.len_utf8()));
    }
    let digit_end = rest.bytes().take(9).take_while(u8::is_ascii_digit).count();
    if digit_end == 0
        || rest
            .as_bytes()
            .get(digit_end)
            .is_some_and(u8::is_ascii_digit)
    {
        return None;
    }
    let delimiter = rest[digit_end..].chars().next()?;
    matches!(delimiter, '.' | ')').then_some((
        Marker::Ordered {
            number: &rest[..digit_end],
            delimiter,
        },
        digit_end + delimiter.len_utf8(),
    ))
}

fn parse_task(after_marker: &str) -> Option<(&str, &str)> {
    let task = after_marker.get(..3)?;
    if !matches!(task, "[ ]" | "[x]" | "[X]") {
        return None;
    }
    let spacing_end = 3 + whitespace_prefix(&after_marker[3..]);
    (spacing_end > 3).then_some((&after_marker[3..spacing_end], &after_marker[spacing_end..]))
}

pub(crate) fn whitespace_prefix(value: &str) -> usize {
    value
        .char_indices()
        .find_map(|(index, character)| (!matches!(character, ' ' | '\t')).then_some(index))
        .unwrap_or(value.len())
}

pub(crate) fn indentation_columns(indentation: &str) -> usize {
    indentation.chars().fold(0, |column, character| {
        let mut buffer = [0; 4];
        column
            + crate::ports::text_layout::grapheme_cell_width(
                character.encode_utf8(&mut buffer),
                column,
            )
    })
}

pub(crate) fn is_thematic_break(line: &str) -> bool {
    let compact = line
        .trim_matches([' ', '\t'])
        .chars()
        .filter(|character| !matches!(character, ' ' | '\t'))
        .collect::<String>();
    compact.len() >= 3
        && compact.chars().next().is_some_and(|marker| {
            matches!(marker, '-' | '*' | '_')
                && compact.chars().all(|character| character == marker)
        })
}

#[derive(Clone, Copy)]
pub(crate) struct FenceState(Option<(char, usize)>);

impl FenceState {
    pub(crate) const fn closed() -> Self {
        Self(None)
    }

    pub(crate) const fn is_open(self) -> bool {
        self.0.is_some()
    }

    pub(crate) fn update(&mut self, text: &str) -> bool {
        let Some((marker, count, trailing)) = fence(text) else {
            return false;
        };
        match self.0 {
            None => self.0 = Some((marker, count)),
            Some((open_marker, minimum))
                if marker == open_marker
                    && count >= minimum
                    && trailing.trim_matches([' ', '\t']).is_empty() =>
            {
                self.0 = None;
            }
            Some(_) => {}
        }
        true
    }
}

fn fence(line: &str) -> Option<(char, usize, &str)> {
    let indentation = whitespace_prefix(line);
    let prefix = &line[..indentation];
    if indentation_columns(prefix) > 3 {
        return None;
    }
    let rest = &line[indentation..];
    let marker = rest.chars().next()?;
    if !matches!(marker, '`' | '~') {
        return None;
    }
    let count = rest
        .chars()
        .take_while(|character| *character == marker)
        .count();
    (count >= 3).then_some((marker, count, &rest[count..]))
}
