//! Field-level JSON value diffing for canonical request diagnostics.
//!
//! This module provides a minimal diff utility that compares two
//! `serde_json::Value` trees and produces a human-readable summary of the
//! differences. It is designed specifically for generating mismatch diagnostics
//! in replay mode, not as a general-purpose JSON diff engine.

use serde_json::Value;

/// Produces a human-readable field-level diff summary comparing two canonical
/// request JSON values.
///
/// Reports keys that are present in only one value (added/removed) and keys
/// whose values differ (changed). Nested objects are compared recursively with
/// dotted path notation.
///
/// # Format
///
/// The output is a newline-separated list of change descriptions:
/// - `added: <path>: <value>` — field present in observed but not expected
/// - `removed: <path>` — field present in expected but not observed
/// - `changed: <path>: <expected_value> -> <observed_value>` — differing values
///
/// # Examples
///
/// ```rust,ignore
/// use serde_json::json;
/// let expected = json!({"method": "POST", "path": "/api"});
/// let observed = json!({"method": "GET", "path": "/api"});
/// let diff = canonical_diff_summary(&expected, &observed);
/// assert!(diff.contains("changed: method"));
/// assert!(diff.contains("POST"));
/// assert!(diff.contains("GET"));
/// ```
pub(crate) fn canonical_diff_summary(expected: &Value, observed: &Value) -> String {
    let mut changes = Vec::new();
    diff_recursive(expected, observed, "", &mut changes);
    changes.join("\n")
}

fn diff_recursive(expected: &Value, observed: &Value, path: &str, changes: &mut Vec<String>) {
    match (expected, observed) {
        (Value::Object(exp_map), Value::Object(obs_map)) => {
            diff_objects(exp_map, obs_map, path, changes);
        }
        (Value::Array(exp_arr), Value::Array(obs_arr)) => {
            diff_arrays(exp_arr, obs_arr, path, changes);
        }
        _ => {
            diff_scalar_or_type_mismatch(expected, observed, path, changes);
        }
    }
}

fn diff_objects(
    exp_map: &serde_json::Map<String, Value>,
    obs_map: &serde_json::Map<String, Value>,
    path: &str,
    changes: &mut Vec<String>,
) {
    let exp_keys: std::collections::HashSet<_> = exp_map.keys().collect();
    let obs_keys: std::collections::HashSet<_> = obs_map.keys().collect();

    // Keys removed (in expected but not observed).
    for key in exp_keys.difference(&obs_keys) {
        let field_path = format_path(path, key);
        changes.push(format!("removed: {field_path}"));
    }

    // Keys added (in observed but not expected).
    for key in obs_keys.difference(&exp_keys) {
        let field_path = format_path(path, key);
        if let Some(value) = obs_map.get(*key) {
            let value_str = compact_value_string(value);
            changes.push(format!("added: {field_path}: {value_str}"));
        }
    }

    // Keys present in both — recurse if values differ.
    for key in exp_keys.intersection(&obs_keys) {
        if let (Some(exp_val), Some(obs_val)) = (exp_map.get(*key), obs_map.get(*key))
            && exp_val != obs_val
        {
            let field_path = format_path(path, key);
            diff_recursive(exp_val, obs_val, &field_path, changes);
        }
    }
}

fn diff_arrays(exp_arr: &[Value], obs_arr: &[Value], path: &str, changes: &mut Vec<String>) {
    let max_len = exp_arr.len().max(obs_arr.len());
    for i in 0..max_len {
        let elem_path = format_path(path, &format!("[{i}]"));
        diff_array_element(exp_arr.get(i), obs_arr.get(i), &elem_path, changes);
    }
}

fn diff_array_element(
    exp_elem: Option<&Value>,
    obs_elem: Option<&Value>,
    elem_path: &str,
    changes: &mut Vec<String>,
) {
    match (exp_elem, obs_elem) {
        (Some(exp), Some(obs)) if exp != obs => {
            diff_recursive(exp, obs, elem_path, changes);
        }
        (Some(_), None) => {
            changes.push(format!("removed: {elem_path}"));
        }
        (None, Some(obs)) => {
            let value_str = compact_value_string(obs);
            changes.push(format!("added: {elem_path}: {value_str}"));
        }
        (Some(_), Some(_)) | (None, None) => {
            // Either equal or both None; both cases are no-ops
        }
    }
}

fn diff_scalar_or_type_mismatch(
    expected: &Value,
    observed: &Value,
    path: &str,
    changes: &mut Vec<String>,
) {
    let exp_str = compact_value_string(expected);
    let obs_str = compact_value_string(observed);
    let display_path = if path.is_empty() {
        "(root)".to_owned()
    } else {
        path.to_owned()
    };
    changes.push(format!("changed: {display_path}: {exp_str} -> {obs_str}"));
}

fn format_path(prefix: &str, segment: &str) -> String {
    if prefix.is_empty() {
        segment.to_owned()
    } else if segment.starts_with('[') {
        // Array index notation — no separator.
        format!("{prefix}{segment}")
    } else {
        // Object key — use dot separator.
        format!("{prefix}.{segment}")
    }
}

fn compact_value_string(value: &Value) -> String {
    match value {
        Value::String(s) => format!("\"{s}\""),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => "null".to_owned(),
        Value::Array(_) => "[...]".to_owned(),
        Value::Object(_) => "{...}".to_owned(),
    }
}
