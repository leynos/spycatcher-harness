//! JSON canonicalization helpers for request hashing.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Number, Value, json};

use super::CanonicalError;
use super::CanonicalRequest;
use super::hex::{HexCase, push_hex_u32};

pub(super) fn canonicalize_body(
    body: Value,
    ignored_body_paths: &[String],
) -> Result<Value, CanonicalError> {
    let mut canonical = body;
    for pointer_tokens in ordered_pointer_removals(ignored_body_paths)? {
        let pointer_path = pointer_tokens_to_path(&pointer_tokens);
        remove_pointer(&mut canonical, &pointer_tokens, &pointer_path)?;
    }

    Ok(sort_json_value(canonical))
}

pub(super) fn is_valid_json_pointer(path: &str) -> bool {
    parse_json_pointer(path).is_some_and(|tokens| {
        !tokens
            .iter()
            .any(|token| looks_like_invalid_array_index(token.as_str()))
    })
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

fn parse_json_pointer(path: &str) -> Option<Vec<PointerToken>> {
    if path.is_empty() || !path.starts_with('/') {
        return None;
    }

    path.split('/')
        .skip(1)
        .map(unescape_pointer_token)
        .collect::<Option<Vec<_>>>()
}

fn unescape_pointer_token(token: &str) -> Option<PointerToken> {
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

    Some(PointerToken(unescaped))
}

fn build_grouped_removals(
    removals: &[PointerRemoval],
) -> BTreeMap<Vec<PointerToken>, Vec<Vec<PointerToken>>> {
    let mut grouped = BTreeMap::<Vec<PointerToken>, Vec<Vec<PointerToken>>>::new();
    for removal in removals {
        if let Some(parent_tokens) = &removal.parent_array {
            grouped
                .entry(parent_tokens.clone())
                .or_default()
                .push(removal.tokens.clone());
        }
    }
    grouped
}

fn emit_grouped_entries(
    parent_tokens: Vec<PointerToken>,
    ordered: &mut Vec<Vec<PointerToken>>,
    grouped: &mut BTreeMap<Vec<PointerToken>, Vec<Vec<PointerToken>>>,
    emitted: &mut BTreeSet<Vec<PointerToken>>,
) {
    if let Some(entries) = grouped.remove(&parent_tokens)
        && emitted.insert(parent_tokens)
    {
        ordered.extend(entries);
    }
}

fn emit_removal(
    removal: PointerRemoval,
    ordered: &mut Vec<Vec<PointerToken>>,
    grouped: &mut BTreeMap<Vec<PointerToken>, Vec<Vec<PointerToken>>>,
    emitted: &mut BTreeSet<Vec<PointerToken>>,
) {
    match removal.parent_array {
        Some(parent_tokens) => emit_grouped_entries(parent_tokens, ordered, grouped, emitted),
        None => ordered.push(removal.tokens),
    }
}

fn ordered_pointer_removals(
    ignored_body_paths: &[String],
) -> Result<Vec<Vec<PointerToken>>, CanonicalError> {
    let removals = ignored_body_paths
        .iter()
        .map(|path| parse_valid_json_pointer(path).map(PointerRemoval::new))
        .collect::<Result<Vec<_>, _>>()?;
    let deduplicated_removals = deduplicate_removals(removals);
    let mut grouped_removals = build_grouped_removals(&deduplicated_removals);

    for entries in grouped_removals.values_mut() {
        entries.sort_unstable_by_key(|entry| std::cmp::Reverse(array_entry_index(entry)));
    }

    let mut ordered = Vec::with_capacity(deduplicated_removals.len());
    let mut emitted_groups = BTreeSet::new();
    for removal in deduplicated_removals {
        emit_removal(
            removal,
            &mut ordered,
            &mut grouped_removals,
            &mut emitted_groups,
        );
    }

    ordered.sort_by_key(|tokens| std::cmp::Reverse(tokens.len()));

    Ok(ordered)
}

fn parse_valid_json_pointer(path: &str) -> Result<Vec<PointerToken>, CanonicalError> {
    parse_json_pointer(path).ok_or_else(|| CanonicalError::InvalidPointerPath(path.to_owned()))
}

fn whole_array_entry_parent(tokens: &[PointerToken]) -> Option<Vec<PointerToken>> {
    let (_, parent_tokens) = tokens.split_last()?;
    array_entry_index(tokens)?;

    Some(parent_tokens.to_vec())
}

fn array_entry_index(tokens: &[PointerToken]) -> Option<usize> {
    parse_array_index_token(tokens.last()?.as_str())
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct PointerToken(String);

impl PointerToken {
    fn as_str(&self) -> &str {
        &self.0
    }
}

fn pointer_tokens_to_path(tokens: &[PointerToken]) -> String {
    let mut path = String::new();
    for token in tokens {
        path.push('/');
        path.push_str(
            token
                .as_str()
                .replace('~', "~0")
                .replace('/', "~1")
                .as_str(),
        );
    }
    path
}

fn deduplicate_removals(removals: Vec<PointerRemoval>) -> Vec<PointerRemoval> {
    let mut seen = BTreeSet::new();
    let mut deduplicated = Vec::with_capacity(removals.len());
    for removal in removals {
        if seen.insert(removal.tokens.clone()) {
            deduplicated.push(removal);
        }
    }
    deduplicated
}

fn parse_array_index_token(token: &str) -> Option<usize> {
    if token == "0" {
        return Some(0);
    }

    let mut digits = token.bytes();
    let first = digits.next()?;
    if !first.is_ascii_digit() || first == b'0' {
        return None;
    }

    if !digits.clone().all(|digit| digit.is_ascii_digit()) {
        return None;
    }

    token.parse::<usize>().ok()
}

fn looks_like_invalid_array_index(token: &str) -> bool {
    token.len() > 1 && token.starts_with('0') && token.bytes().all(|digit| digit.is_ascii_digit())
}

struct PointerRemoval {
    tokens: Vec<PointerToken>,
    parent_array: Option<Vec<PointerToken>>,
}

impl PointerRemoval {
    fn new(tokens: Vec<PointerToken>) -> Self {
        let parent_array = whole_array_entry_parent(&tokens);
        Self {
            tokens,
            parent_array,
        }
    }
}

fn remove_pointer(
    value: &mut Value,
    tokens: &[PointerToken],
    pointer_path: &str,
) -> Result<(), CanonicalError> {
    let Some((head, tail)) = tokens.split_first() else {
        return Ok(());
    };

    match value {
        Value::Object(object) => remove_object_pointer(object, head, tail, pointer_path),
        Value::Array(items) => remove_array_pointer(items, head, tail, pointer_path),
        _ => Ok(()),
    }
}

fn remove_object_pointer(
    object: &mut Map<String, Value>,
    head: &PointerToken,
    tail: &[PointerToken],
    pointer_path: &str,
) -> Result<(), CanonicalError> {
    if tail.is_empty() {
        object.remove(head.as_str());
        return Ok(());
    }

    if let Some(next) = object.get_mut(head.as_str()) {
        return remove_pointer(next, tail, pointer_path);
    }

    Ok(())
}

fn remove_array_pointer(
    items: &mut Vec<Value>,
    head: &PointerToken,
    tail: &[PointerToken],
    pointer_path: &str,
) -> Result<(), CanonicalError> {
    let Some(index) = parse_array_index_token(head.as_str()) else {
        return Err(CanonicalError::InvalidPointerPath(pointer_path.to_owned()));
    };

    if tail.is_empty() {
        remove_array_entry(items, index);
        return Ok(());
    }

    if let Some(next) = items.get_mut(index) {
        return remove_pointer(next, tail, pointer_path);
    }

    Ok(())
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
