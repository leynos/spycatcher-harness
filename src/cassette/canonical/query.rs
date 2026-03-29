//! Query string canonicalization helpers.

use super::hex::{HexCase, decode_hex_digit, push_hex_byte};

pub(super) fn canonicalize_query(query: &str) -> String {
    if query.is_empty() {
        return String::new();
    }

    let mut pairs = parse_query_pairs(query);
    pairs.sort_unstable();
    encode_query_pairs(&pairs)
}

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

fn percent_decode_canonical(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;

    while let Some(current_byte) = bytes.get(index).copied() {
        match current_byte {
            b'%' => {
                let high_digit = bytes.get(index + 1).copied().and_then(decode_hex_digit);
                let low_digit = bytes.get(index + 2).copied().and_then(decode_hex_digit);
                if let (Some(high_nibble), Some(low_nibble)) = (high_digit, low_digit) {
                    output.push((high_nibble << 4) | low_nibble);
                    index += 3;
                } else {
                    output.push(current_byte);
                    index += 1;
                }
            }
            byte => {
                output.push(byte);
                index += 1;
            }
        }
    }

    let mut canonical = String::with_capacity(output.len());
    for byte in output {
        if is_unreserved(byte) {
            canonical.push(char::from(byte));
        } else {
            canonical.push('%');
            push_hex_byte(&mut canonical, byte, HexCase::Upper);
        }
    }

    canonical
}

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

const fn is_unreserved(byte: u8) -> bool {
    matches!(
        byte,
        b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~'
    )
}
