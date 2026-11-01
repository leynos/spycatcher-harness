//! Writes canonical JSON values with stable escaping and ordering.

use serde_json::{Number, Value};

use super::super::hex::{HexCase, push_hex_u32};

/// Appends compact JSON; serializing `null` adds the text `null` to the output.
pub(super) fn write_json_canonical(output: &mut String, value: &Value) {
    match value {
        Value::Null => output.push_str("null"),
        Value::Bool(flag) => output.push_str(if *flag { "true" } else { "false" }),
        Value::Number(number) => write_json_number(output, number),
        Value::String(text) => write_json_string(output, text),
        Value::Array(items) => write_json_array(output, items),
        Value::Object(object) => write_json_object(output, object),
    }
}

/// Writes an array without spaces; `[1, 2]` becomes `[1,2]`.
fn write_json_array(output: &mut String, items: &[Value]) {
    output.push('[');
    let mut iter = items.iter();
    if let Some(first) = iter.next() {
        write_json_canonical(output, first);
        for item in iter {
            output.push(',');
            write_json_canonical(output, item);
        }
    }
    output.push(']');
}

/// Writes object keys and values without spaces; `{"a": 1}` becomes `{"a":1}`.
fn write_json_object(output: &mut String, object: &serde_json::Map<String, Value>) {
    output.push('{');
    let mut iter = object.iter();
    if let Some((first_key, first_val)) = iter.next() {
        write_json_string(output, first_key);
        output.push(':');
        write_json_canonical(output, first_val);
        for (key, entry_value) in iter {
            output.push(',');
            write_json_string(output, key);
            output.push(':');
            write_json_canonical(output, entry_value);
        }
    }
    output.push('}');
}

/// Appends `serde_json`'s numeric representation; `1.25` remains `1.25`.
fn write_json_number(output: &mut String, number: &Number) { output.push_str(&number.to_string()); }

/// Quotes and escapes a JSON string; a newline is written as `\n`.
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
                push_hex_u32(output, u32::from(control_char), HexCase::Lower);
            }
            other => output.push(other),
        }
    }
    output.push('"');
}
