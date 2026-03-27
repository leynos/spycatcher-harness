//! Query string canonicalization helpers.

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
            (percent_decode(key), percent_decode(value))
        })
        .collect()
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;

    while let Some(current_byte) = bytes.get(index).copied() {
        match current_byte {
            b'+' => {
                output.push(b' ');
                index += 1;
            }
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

    String::from_utf8_lossy(&output).into_owned()
}

const fn decode_hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn encode_query_pairs(pairs: &[(String, String)]) -> String {
    let mut output = String::new();
    for (index, (key, value)) in pairs.iter().enumerate() {
        if index > 0 {
            output.push('&');
        }
        output.push_str(&percent_encode(key));
        output.push('=');
        output.push_str(&percent_encode(value));
    }
    output
}

fn percent_encode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut output = String::with_capacity(bytes.len());

    for byte in bytes {
        if is_unreserved(*byte) {
            output.push(char::from(*byte));
        } else {
            output.push('%');
            push_hex_nibble(&mut output, byte >> 4);
            push_hex_nibble(&mut output, byte & 0x0F);
        }
    }

    output
}

const fn is_unreserved(byte: u8) -> bool {
    matches!(
        byte,
        b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~'
    )
}

fn push_hex_nibble(output: &mut String, nibble: u8) {
    let hex = match nibble & 0x0F {
        0 => '0',
        1 => '1',
        2 => '2',
        3 => '3',
        4 => '4',
        5 => '5',
        6 => '6',
        7 => '7',
        8 => '8',
        9 => '9',
        10 => 'a',
        11 => 'b',
        12 => 'c',
        13 => 'd',
        14 => 'e',
        _ => 'f',
    };
    output.push(hex);
}
