//! JSON canonicalization helpers for request hashing.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Number, Value, json};

use super::CanonicalRequest;
use super::hex::{HexCase, push_hex_u32};

pub(super) fn canonicalize_body(body: Value, ignored_body_paths: &[String]) -> Value {
    let mut canonical = body;
    for pointer_tokens in ordered_pointer_removals(ignored_body_paths) {
        remove_pointer(&mut canonical, &pointer_tokens);
    }

    sort_json_value(canonical)
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
    if path.is_empty() || !path.starts_with('/') {
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

fn ordered_pointer_removals(ignored_body_paths: &[String]) -> Vec<Vec<String>> {
    let removals: Vec<PointerRemoval> = ignored_body_paths
        .iter()
        .filter_map(|path| parse_json_pointer(path))
        .map(PointerRemoval::new)
        .collect();
    let mut grouped_removals = BTreeMap::<Vec<String>, Vec<Vec<String>>>::new();

    for removal in &removals {
        if let Some(parent_tokens) = &removal.parent_array {
            grouped_removals
                .entry(parent_tokens.clone())
                .or_default()
                .push(removal.tokens.clone());
        }
    }

    for entries in grouped_removals.values_mut() {
        entries.sort_unstable_by_key(|entry| std::cmp::Reverse(array_entry_index(entry)));
    }

    let mut ordered = Vec::with_capacity(removals.len());
    let mut emitted_groups = BTreeSet::new();
    for removal in removals {
        match removal.parent_array {
            Some(parent_tokens) => {
                if emitted_groups.insert(parent_tokens.clone())
                    && let Some(entries) = grouped_removals.remove(&parent_tokens)
                {
                    ordered.extend(entries);
                }
            }
            None => ordered.push(removal.tokens),
        }
    }

    ordered
}

fn whole_array_entry_parent(tokens: &[String]) -> Option<Vec<String>> {
    let (_, parent_tokens) = tokens.split_last()?;

    if parent_tokens.is_empty() || array_entry_index(tokens).is_none() {
        return None;
    }

    Some(parent_tokens.to_vec())
}

fn array_entry_index(tokens: &[String]) -> Option<usize> {
    tokens.last()?.parse::<usize>().ok()
}

struct PointerRemoval {
    tokens: Vec<String>,
    parent_array: Option<Vec<String>>,
}

impl PointerRemoval {
    fn new(tokens: Vec<String>) -> Self {
        let parent_array = whole_array_entry_parent(&tokens);
        Self {
            tokens,
            parent_array,
        }
    }
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
        Value::Bool(flag) => output.push_str(if *flag { "true" } else { "false" }),
        Value::Number(number) => write_json_number(output, number),
        Value::String(text) => write_json_string(output, text),
        Value::Array(items) => write_json_array(output, items),
        Value::Object(object) => write_json_object(output, object),
    }
}

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
                push_hex_u32(output, u32::from(control_char), HexCase::Lower);
            }
            other => output.push(other),
        }
    }
    output.push('"');
}
