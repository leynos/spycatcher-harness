//! Query string canonicalization helpers.

use super::hex::{HexCase, decode_hex_digit, push_hex_byte};

/// Sorts decoded query pairs and emits one stable percent-encoded form.
/// For example, `b=2&a=1` becomes `a=1&b=2`.
pub(super) fn canonicalize_query(query: &str) -> String {
    if query.is_empty() {
        return String::new();
    }

    let mut pairs = parse_query_pairs(query);
    pairs.sort_unstable();
    encode_query_pairs(&pairs)
}

/// Parses nonempty query segments, treating a missing equals sign as an empty value.
/// For example, `a&b=2` produces pairs `("a", "")` and `("b", "2")`.
fn parse_query_pairs(query: &str) -> Vec<(String, String)> {
    query
        .split('&')
        .filter(|segment| !segment.is_empty())
        .map(|segment| {
            let (key, value) = segment
                .split_once('=')
                .map_or((segment, ""), |(key, value)| (key, value));
            (
                percent_decode_canonical(key),
                percent_decode_canonical(value),
            )
        })
        .collect()
}

/// Decodes a complete percent escape at `index` into an optional byte.
///
/// For example, `try_decode_percent_at(b"%2F", 0)` returns `Some(b'/')`; `%2` or
/// `%2G` returns `None`. The caller, `decode_percent_bytes`, preserves the current
/// input byte when no valid escape is found.
fn try_decode_percent_at(bytes: &[u8], index: usize) -> Option<u8> {
    if bytes.get(index).copied()? != b'%' {
        return None;
    }
    let high = bytes.get(index + 1).copied().and_then(decode_hex_digit)?;
    let low = bytes.get(index + 2).copied().and_then(decode_hex_digit)?;
    Some((high << 4) | low)
}

/// Converts valid percent escapes to bytes while retaining other input bytes.
/// For example, `a%20b` becomes the bytes for `a b`.
fn decode_percent_bytes(input: &str) -> Vec<u8> {
    let bytes = input.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let Some(current_byte) = bytes.get(index).copied() else {
            break;
        };
        let (byte, advance) =
            try_decode_percent_at(bytes, index).map_or((current_byte, 1), |b| (b, 3));
        output.push(byte);
        index += advance;
    }
    output
}

/// Writes an unreserved byte directly or uses an uppercase percent escape.
/// For example, a space appends `%20`, while `a` appends `a`.
fn push_canonical_byte(canonical: &mut String, byte: u8) {
    if is_unreserved(byte) {
        canonical.push(char::from(byte));
    } else {
        canonical.push('%');
        push_hex_byte(canonical, byte, HexCase::Upper);
    }
}

/// Encodes bytes so every reserved value has one stable representation.
/// For example, bytes for `a b` produce `a%20b`.
fn encode_canonical_bytes(bytes: &[u8]) -> String {
    let mut canonical = String::with_capacity(bytes.len() * 3);
    for &byte in bytes {
        push_canonical_byte(&mut canonical, byte);
    }
    canonical
}

/// Normalizes percent escapes, including case and redundant unreserved escapes.
/// For example, `%61%2f` becomes `a%2F`.
fn percent_decode_canonical(input: &str) -> String {
    encode_canonical_bytes(&decode_percent_bytes(input))
}

/// Joins ordered pairs with explicit equals signs, including empty values.
/// For example, a pair `("a", "")` becomes `a=`.
fn encode_query_pairs(pairs: &[(String, String)]) -> String {
    let mut output = String::new();
    for (index, (key, value)) in pairs.iter().enumerate() {
        if index > 0 {
            output.push('&');
        }
        output.push_str(key);
        output.push('=');
        output.push_str(value);
    }
    output
}

/// Identifies RFC 3986 unreserved bytes that need no percent escape.
/// For example, `b'~'` is unreserved while `b'/'` is reserved.
const fn is_unreserved(byte: u8) -> bool {
    matches!(
        byte,
        b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~'
    )
}
