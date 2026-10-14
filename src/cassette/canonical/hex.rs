//! Shared hex helpers for canonicalization internals.

/// Encodes bytes as lowercase hex for stable request hashes.
/// For example, byte `0xAF` becomes `"af"`.
pub(super) fn encode_hex(bytes: impl AsRef<[u8]>) -> String {
    let bytes_ref = bytes.as_ref();
    let mut output = String::with_capacity(bytes_ref.len() * 2);
    for byte in bytes_ref {
        push_hex_byte(&mut output, *byte, HexCase::Lower);
    }
    output
}

/// Appends both hex digits of a byte in the requested case.
/// For example, `0xAF` in upper case appends `"AF"`.
pub(super) fn push_hex_byte(output: &mut String, byte: u8, case: HexCase) {
    push_hex_nibble(output, byte >> 4, case);
    push_hex_nibble(output, byte & 0x0F, case);
}

/// Appends four hex digits from a Unicode escape value.
/// For example, `0x00E9` appends `"00E9"` in upper case.
pub(super) fn push_hex_u32(output: &mut String, value: u32, case: HexCase) {
    for shift in [12_u32, 8, 4, 0] {
        push_hex_nibble_u32(output, (value >> shift) & 0x0F, case);
    }
}

/// Decodes one ASCII hex digit, rejecting all other bytes.
/// For example, `b'f'` yields `Some(15)` and `b'g'` yields `None`.
pub(super) const fn decode_hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// Letter case used when writing canonical hex digits.
#[derive(Clone, Copy)]
pub(super) enum HexCase {
    /// Lowercase letters for stable hash output.
    Lower,
    /// Uppercase letters for percent escapes.
    Upper,
}

/// Appends one masked nibble as an ASCII hex digit.
/// For example, nibble `10` appends `a` in lower case.
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

/// Narrows a masked Unicode nibble before writing its hex digit.
/// For example, `0x1F` appends `f` in lower case.
fn push_hex_nibble_u32(output: &mut String, nibble: u32, case: HexCase) {
    push_hex_nibble(output, (nibble & 0x0F) as u8, case);
}
