//! JSON canonicalization helpers for request hashing.

use serde_json::{Map, Number, Value, json};

use super::CanonicalRequest;

pub(super) fn canonicalize_body(body: Value, ignored_body_paths: &[String]) -> Option<Value> {
    if ignored_body_paths.iter().any(String::is_empty) {
        return None;
    }

    let mut canonical = sort_json_value(body);
    for path in ignored_body_paths {
        if let Some(pointer_tokens) = parse_json_pointer(path) {
            remove_pointer(&mut canonical, &pointer_tokens);
        }
    }

    Some(sort_json_value(canonical))
}

pub(super) fn serialize_json_canonical(value: &Value) -> String {
    let mut output = String::new();
    write_json_canonical(&mut output, value);
    output
}

pub(super) fn canonical_request_value(canonical: &CanonicalRequest) -> Value {
    json!({
        "method": canonical.method,
        "path": canonical.path,
        "canonical_query": canonical.canonical_query,
        "canonical_body": canonical.canonical_body,
    })
}

pub(super) fn encode_hex(bytes: impl AsRef<[u8]>) -> String {
    let bytes_ref = bytes.as_ref();
    let mut output = String::with_capacity(bytes_ref.len() * 2);
    for byte in bytes_ref {
        push_hex_nibble(&mut output, byte >> 4);
        push_hex_nibble(&mut output, byte & 0x0F);
    }
    output
}

fn sort_json_value(value: Value) -> Value {
    match value {
        Value::Array(values) => Value::Array(values.into_iter().map(sort_json_value).collect()),
        Value::Object(object) => {
            let mut entries: Vec<(String, Value)> = object.into_iter().collect();
            entries.sort_unstable_by(|left, right| left.0.cmp(&right.0));
            let mut sorted = Map::with_capacity(entries.len());
            for (key, entry_value) in entries {
                sorted.insert(key, sort_json_value(entry_value));
            }
            Value::Object(sorted)
        }
        other => other,
    }
}

fn parse_json_pointer(path: &str) -> Option<Vec<String>> {
    if !path.starts_with('/') {
        return None;
    }

    path.split('/')
        .skip(1)
        .map(unescape_pointer_token)
        .collect::<Option<Vec<_>>>()
}

fn unescape_pointer_token(token: &str) -> Option<String> {
    let mut unescaped = String::with_capacity(token.len());
    let mut chars = token.chars();

    while let Some(character) = chars.next() {
        if character == '~' {
            match chars.next() {
                Some('0') => unescaped.push('~'),
                Some('1') => unescaped.push('/'),
                _ => return None,
            }
        } else {
            unescaped.push(character);
        }
    }

    Some(unescaped)
}

fn remove_pointer(value: &mut Value, tokens: &[String]) {
    let Some((head, tail)) = tokens.split_first() else {
        return;
    };

    match value {
        Value::Object(object) => remove_object_pointer(object, head, tail),
        Value::Array(items) => remove_array_pointer(items, head, tail),
        _ => {}
    }
}

fn remove_object_pointer(object: &mut Map<String, Value>, head: &str, tail: &[String]) {
    if tail.is_empty() {
        object.remove(head);
        return;
    }

    if let Some(next) = object.get_mut(head) {
        remove_pointer(next, tail);
    }
}

fn remove_array_pointer(items: &mut Vec<Value>, head: &str, tail: &[String]) {
    let Ok(index) = head.parse::<usize>() else {
        return;
    };

    if tail.is_empty() {
        remove_array_entry(items, index);
        return;
    }

    if let Some(next) = items.get_mut(index) {
        remove_pointer(next, tail);
    }
}

fn remove_array_entry(items: &mut Vec<Value>, index: usize) {
    if index < items.len() {
        items.remove(index);
    }
}

fn write_json_canonical(output: &mut String, value: &Value) {
    match value {
        Value::Null => output.push_str("null"),
        Value::Bool(flag) => {
            if *flag {
                output.push_str("true");
            } else {
                output.push_str("false");
            }
        }
        Value::Number(number) => write_json_number(output, number),
        Value::String(text) => write_json_string(output, text),
        Value::Array(items) => {
            output.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    output.push(',');
                }
                write_json_canonical(output, item);
            }
            output.push(']');
        }
        Value::Object(object) => {
            output.push('{');
            for (index, (key, entry_value)) in object.iter().enumerate() {
                if index > 0 {
                    output.push(',');
                }
                write_json_string(output, key);
                output.push(':');
                write_json_canonical(output, entry_value);
            }
            output.push('}');
        }
    }
}

fn write_json_number(output: &mut String, number: &Number) {
    output.push_str(&number.to_string());
}

fn write_json_string(output: &mut String, text: &str) {
    output.push('"');
    for current_char in text.chars() {
        match current_char {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\u{08}' => output.push_str("\\b"),
            '\u{0C}' => output.push_str("\\f"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            control_char if control_char <= '\u{1F}' => {
                output.push_str("\\u");
                push_hex_u32(output, u32::from(control_char));
            }
            other => output.push(other),
        }
    }
    output.push('"');
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

fn push_hex_u32(output: &mut String, value: u32) {
    for shift in [12_u32, 8, 4, 0] {
        let nibble = (value >> shift) & 0x0F;
        let hex = match nibble {
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
}
