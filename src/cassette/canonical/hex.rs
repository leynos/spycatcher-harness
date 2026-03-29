//! Shared hex helpers for canonicalization internals.

pub(super) fn encode_hex(bytes: impl AsRef<[u8]>) -> String {
    let bytes_ref = bytes.as_ref();
    let mut output = String::with_capacity(bytes_ref.len() * 2);
    for byte in bytes_ref {
        push_hex_byte(&mut output, *byte, HexCase::Lower);
    }
    output
}

pub(super) fn push_hex_byte(output: &mut String, byte: u8, case: HexCase) {
    push_hex_nibble(output, byte >> 4, case);
    push_hex_nibble(output, byte & 0x0F, case);
}

pub(super) fn push_hex_u32(output: &mut String, value: u32, case: HexCase) {
    for shift in [12_u32, 8, 4, 0] {
        push_hex_nibble_u32(output, (value >> shift) & 0x0F, case);
    }
}

pub(super) const fn decode_hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[derive(Clone, Copy)]
pub(super) enum HexCase {
    Lower,
    Upper,
}

fn push_hex_nibble(output: &mut String, nibble: u8, case: HexCase) {
    let hex = match (nibble & 0x0F, case) {
        (0, _) => '0',
        (1, _) => '1',
        (2, _) => '2',
        (3, _) => '3',
        (4, _) => '4',
        (5, _) => '5',
        (6, _) => '6',
        (7, _) => '7',
        (8, _) => '8',
        (9, _) => '9',
        (10, HexCase::Lower) => 'a',
        (11, HexCase::Lower) => 'b',
        (12, HexCase::Lower) => 'c',
        (13, HexCase::Lower) => 'd',
        (14, HexCase::Lower) => 'e',
        (15, HexCase::Lower) => 'f',
        (10, HexCase::Upper) => 'A',
        (11, HexCase::Upper) => 'B',
        (12, HexCase::Upper) => 'C',
        (13, HexCase::Upper) => 'D',
        (14, HexCase::Upper) => 'E',
        _ => 'F',
    };
    output.push(hex);
}

fn push_hex_nibble_u32(output: &mut String, nibble: u32, case: HexCase) {
    let hex = match (nibble & 0x0F, case) {
        (0, _) => '0',
        (1, _) => '1',
        (2, _) => '2',
        (3, _) => '3',
        (4, _) => '4',
        (5, _) => '5',
        (6, _) => '6',
        (7, _) => '7',
        (8, _) => '8',
        (9, _) => '9',
        (10, HexCase::Lower) => 'a',
        (11, HexCase::Lower) => 'b',
        (12, HexCase::Lower) => 'c',
        (13, HexCase::Lower) => 'd',
        (14, HexCase::Lower) => 'e',
        (15, HexCase::Lower) => 'f',
        (10, HexCase::Upper) => 'A',
        (11, HexCase::Upper) => 'B',
        (12, HexCase::Upper) => 'C',
        (13, HexCase::Upper) => 'D',
        (14, HexCase::Upper) => 'E',
        _ => 'F',
    };
    output.push(hex);
}
