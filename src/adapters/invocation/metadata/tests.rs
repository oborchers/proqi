use std::io::{self, Cursor, Read};

use super::{MAX_MARKDOWN_FRONTMATTER_BYTES, MarkdownMetadata, markdown_reader};

struct ReadProbe {
    bytes: Cursor<Vec<u8>>,
    bytes_read: usize,
    largest_request: usize,
}

impl ReadProbe {
    fn new(bytes: Vec<u8>) -> Self {
        Self {
            bytes: Cursor::new(bytes),
            bytes_read: 0,
            largest_request: 0,
        }
    }
}

impl Read for ReadProbe {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.largest_request = self.largest_request.max(buffer.len());
        let count = self.bytes.read(buffer)?;
        self.bytes_read = self.bytes_read.saturating_add(count);
        Ok(count)
    }
}

#[test]
fn production_reader_stops_underlying_reads_at_the_closing_delimiter() {
    let prefix = b"---\nname: bounded\ndescription: Prefix only\n---\n";
    let mut definition = prefix.to_vec();
    definition.extend_from_slice(b"instruction body\n\xff\xfe");
    let mut reader = ReadProbe::new(definition);

    let MarkdownMetadata::Parsed(metadata) = markdown_reader(&mut reader) else {
        panic!("valid metadata prefix");
    };

    assert_eq!(metadata.name.as_deref(), Some("bounded"));
    assert_eq!(metadata.description.as_deref(), Some("Prefix only"));
    assert_eq!(reader.bytes_read, prefix.len());
    assert_eq!(reader.bytes.position(), prefix.len() as u64);
    assert_eq!(reader.largest_request, 1);
}

#[test]
fn filename_derived_command_without_frontmatter_reads_only_the_first_byte() {
    let mut reader = Cursor::new(b"instruction body that is not metadata".as_slice());

    assert!(matches!(
        markdown_reader(&mut reader),
        MarkdownMetadata::Absent
    ));
    assert_eq!(reader.position(), 1);
}

#[test]
fn invalid_utf8_inside_metadata_fails_closed() {
    let mut reader = Cursor::new(b"---\ndescription: invalid \xff\n---\nbody".as_slice());

    assert!(matches!(
        markdown_reader(&mut reader),
        MarkdownMetadata::Invalid
    ));
}

#[test]
fn crlf_frontmatter_is_bounded_by_its_closing_line() {
    let prefix = b"---\r\nname: crlf\r\ndescription: Windows lines\r\n---\r\n";
    let mut definition = prefix.to_vec();
    definition.extend_from_slice(b"body\r\n");
    let mut reader = Cursor::new(definition);

    let MarkdownMetadata::Parsed(metadata) = markdown_reader(&mut reader) else {
        panic!("CRLF metadata");
    };

    assert_eq!(metadata.name.as_deref(), Some("crlf"));
    assert_eq!(metadata.description.as_deref(), Some("Windows lines"));
    assert_eq!(reader.position(), prefix.len() as u64);
}

#[test]
fn unterminated_frontmatter_stops_at_the_budget() {
    let mut definition = b"---\ndescription: never closed\n".to_vec();
    definition.resize(MAX_MARKDOWN_FRONTMATTER_BYTES + 8_192, b'x');
    let mut reader = Cursor::new(definition);

    assert!(matches!(
        markdown_reader(&mut reader),
        MarkdownMetadata::Invalid
    ));
    assert_eq!(reader.position(), MAX_MARKDOWN_FRONTMATTER_BYTES as u64);
}

#[test]
fn closing_delimiter_beyond_the_budget_fails_closed() {
    let mut definition = b"---\nmetadata-padding: ".to_vec();
    definition.resize(MAX_MARKDOWN_FRONTMATTER_BYTES + 1, b'x');
    definition.extend_from_slice(b"\n---\nbody");
    let mut reader = Cursor::new(definition);

    assert!(matches!(
        markdown_reader(&mut reader),
        MarkdownMetadata::Invalid
    ));
    assert_eq!(reader.position(), MAX_MARKDOWN_FRONTMATTER_BYTES as u64);
}

#[test]
fn metadata_fields_after_the_line_bound_are_not_retained() {
    let mut definition = String::from("---\n");
    definition.push_str(&"ignored: value\n".repeat(64));
    definition.push_str("description: too late\n---\nbody");
    let mut reader = Cursor::new(definition.as_bytes());

    let MarkdownMetadata::Parsed(metadata) = markdown_reader(&mut reader) else {
        panic!("closed metadata");
    };

    assert!(metadata.description.is_none());
}
