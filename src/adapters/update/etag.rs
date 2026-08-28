//! Shared bounded HTTP entity-tag validation.

const MAX_ETAG_BYTES: usize = 256;

pub(super) fn valid(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_ETAG_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_graphic() || byte == b' ')
        && !value.bytes().any(|byte| matches!(byte, b'\r' | b'\n'))
}
