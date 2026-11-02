//! JSON canonicalization helpers for request hashing.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value, json};

use super::{CanonicalError, CanonicalRequest};

mod serialize;
use self::serialize::write_json_canonical;

/// Prepares a body for hashing by removing ignored paths and sorting object keys.
///
/// For example, `canonicalize_body(json!({"secret": "token", "model": "demo"}),
/// &["/secret".to_owned()])` returns `Ok(json!({"model": "demo"}))`, so the secret
/// contributes neither to the canonical body nor its hash.
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

/// Rejects malformed escapes and zero-padded decimal tokens; `/rows/01` is invalid.
pub(super) fn is_valid_json_pointer(path: &str) -> bool {
    parse_json_pointer(path).is_some_and(|tokens| {
        !tokens
            .iter()
            .any(|token| looks_like_invalid_array_index(token.as_str()))
    })
}

/// Serializes an already-canonical JSON value without whitespace.
///
/// For example, first obtain `canonical` from
/// `canonicalize_body(json!({"b": 2, "a": 1}), &[])?`; then
/// `serialize_json_canonical(&canonical)` returns `r#"{"a":1,"b":2}"#`.
/// Canonicalization establishes the key order; serialization retains it.
pub(super) fn serialize_json_canonical(value: &Value) -> String {
    let mut output = String::new();
    write_json_canonical(&mut output, value);
    output
}

/// Builds the JSON object for a canonical request, retaining all four fields.
///
/// For example, a `CanonicalRequest` with method `"GET"`, path `"/health"`, an
/// empty `canonical_query` and `canonical_body: None` produces
/// `json!({"method": "GET", "path": "/health", "canonical_query": "",
/// "canonical_body": null})` through `canonical_request_value(&request)`.
pub(super) fn canonical_request_value(canonical: &CanonicalRequest) -> Value {
    json!({
        "method": canonical.method,
        "path": canonical.path,
        "canonical_query": canonical.canonical_query,
        "canonical_body": canonical.canonical_body,
    })
}

/// Recursively sorts object keys; `{"z":0,"a":1}` becomes `{"a":1,"z":0}`.
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

/// Parses a slash-prefixed JSON Pointer; `/a~1b` yields one token, `a/b`.
fn parse_json_pointer(path: &str) -> Option<Vec<PointerToken>> {
    if path.is_empty() || !path.starts_with('/') {
        return None;
    }

    path.split('/')
        .skip(1)
        .map(unescape_pointer_token)
        .collect::<Option<Vec<_>>>()
}

/// Decodes a JSON Pointer token; for example, `~0~1` becomes `~/`.
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

/// Groups whole-array-entry removals by parent; `/rows/2` is grouped under `/rows`.
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

/// Emits an array's grouped removals once, with indices such as `3` before `2`.
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

/// Emits an array group together or appends a standalone removal; `/rows/2` selects its group.
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

/// Orders removals deepest-first and array indices descending; `/rows/2` precedes `/rows/1`.
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

/// Converts malformed pointers into `CanonicalError`; `/a~2b` returns `InvalidPointerPath`.
fn parse_valid_json_pointer(path: &str) -> Result<Vec<PointerToken>, CanonicalError> {
    parse_json_pointer(path).ok_or_else(|| CanonicalError::InvalidPointerPath(path.to_owned()))
}

/// Finds the parent of a whole-array entry; `/rows/3` yields `/rows`.
fn whole_array_entry_parent(tokens: &[PointerToken]) -> Option<Vec<PointerToken>> {
    let (_, parent_tokens) = tokens.split_last()?;
    array_entry_index(tokens)?;

    Some(parent_tokens.to_vec())
}

/// Parses the final token as an array index; `rows`, `3` yields `Some(3)`.
fn array_entry_index(tokens: &[PointerToken]) -> Option<usize> {
    parse_array_index_token(tokens.last()?.as_str())
}

/// Stores one decoded JSON Pointer segment; `/a~1b` is stored as `a/b`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct PointerToken(String);

impl PointerToken {
    /// Borrows the decoded segment; stored `a/b` is returned without pointer escaping.
    fn as_str(&self) -> &str { &self.0 }
}

/// Formats decoded tokens as an escaped JSON Pointer; `a/b` becomes `/a~1b`.
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

/// Keeps the first occurrence of each removal; duplicate `/id` paths leave one entry.
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

/// Parses canonical array indices; `0` and `12` pass, while `01` does not.
fn parse_array_index_token(token: &str) -> Option<usize> {
    if token == "0" {
        return Some(0);
    }

    let mut digits = token.bytes();
    let first = digits.next()?;
    if !first.is_ascii_digit() || first == b'0' {
        return None;
    }

    if !digits.all(|digit| digit.is_ascii_digit()) {
        return None;
    }

    token.parse::<usize>().ok()
}

/// Detects multi-digit decimal tokens with leading zeroes; `01` is ambiguous.
fn looks_like_invalid_array_index(token: &str) -> bool {
    token.len() > 1 && token.starts_with('0') && token.bytes().all(|digit| digit.is_ascii_digit())
}

/// Pairs pointer tokens with parent metadata for whole-array-entry removals.
struct PointerRemoval {
    /// Holds decoded pointer segments; `/rows/2` is stored as `rows`, then `2`.
    tokens: Vec<PointerToken>,
    /// Holds a parent only for a whole array entry; `/rows/2` records `rows`.
    parent_array: Option<Vec<PointerToken>>,
}

impl PointerRemoval {
    /// Builds removal metadata from decoded tokens; `/rows/2` gains parent `rows`.
    fn new(tokens: Vec<PointerToken>) -> Self {
        let parent_array = whole_array_entry_parent(&tokens);
        Self {
            tokens,
            parent_array,
        }
    }
}

/// Recurses through objects and arrays to remove a path; `/user/name` removes that nested key.
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

/// Removes or descends through an object key; a missing `/user/name` path is a no-op.
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

/// Removes or descends through an indexed entry; a non-canonical index token returns an error.
///
/// For example, with `items = [10, 20]` as JSON values and `head` representing
/// `"1"`, `remove_array_pointer(&mut items, &head, &[], "/1")` returns `Ok(())`
/// and leaves `[10]`. A head representing `"01"` instead returns
/// `CanonicalError::InvalidPointerPath("/1".to_owned())` without changing items.
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

/// Removes an entry when its index is in range; index `1` removes the second value.
fn remove_array_entry(items: &mut Vec<Value>, index: usize) {
    if index < items.len() {
        items.remove(index);
    }
}
