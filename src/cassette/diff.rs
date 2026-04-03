//! Field-level JSON value diffing for canonical request diagnostics.
//!
//! This module provides a minimal diff utility that compares two
//! `serde_json::Value` trees and produces a human-readable summary of the
//! differences. It is designed specifically for generating mismatch diagnostics
//! in replay mode, not as a general-purpose JSON diff engine.

#![allow(dead_code)] // Used by matching engine in next stage.

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
    diff_recursive(expected, observed, String::new(), &mut changes);
    changes.join("\n")
}

fn diff_recursive(expected: &Value, observed: &Value, path: String, changes: &mut Vec<String>) {
    match (expected, observed) {
        (Value::Object(exp_map), Value::Object(obs_map)) => {
            // Find keys in both, only in expected, and only in observed.
            let exp_keys: std::collections::HashSet<_> = exp_map.keys().collect();
            let obs_keys: std::collections::HashSet<_> = obs_map.keys().collect();

            // Keys removed (in expected but not observed).
            for key in exp_keys.difference(&obs_keys) {
                let field_path = format_path(&path, key);
                changes.push(format!("removed: {field_path}"));
            }

            // Keys added (in observed but not expected).
            for key in obs_keys.difference(&exp_keys) {
                let field_path = format_path(&path, key);
                let value_str = compact_value_string(&obs_map[*key]);
                changes.push(format!("added: {field_path}: {value_str}"));
            }

            // Keys present in both — recurse if values differ.
            for key in exp_keys.intersection(&obs_keys) {
                let field_path = format_path(&path, key);
                let exp_val = &exp_map[*key];
                let obs_val = &obs_map[*key];
                if exp_val != obs_val {
                    diff_recursive(exp_val, obs_val, field_path, changes);
                }
            }
        }
        (Value::Array(exp_arr), Value::Array(obs_arr)) => {
            let max_len = exp_arr.len().max(obs_arr.len());
            for i in 0..max_len {
                let elem_path = format_path(&path, &format!("[{i}]"));
                match (exp_arr.get(i), obs_arr.get(i)) {
                    (Some(exp_elem), Some(obs_elem)) => {
                        if exp_elem != obs_elem {
                            diff_recursive(exp_elem, obs_elem, elem_path, changes);
                        }
                    }
                    (Some(_), None) => {
                        changes.push(format!("removed: {elem_path}"));
                    }
                    (None, Some(obs_elem)) => {
                        let value_str = compact_value_string(obs_elem);
                        changes.push(format!("added: {elem_path}: {value_str}"));
                    }
                    (None, None) => unreachable!(),
                }
            }
        }
        _ => {
            // Scalar or type mismatch.
            let exp_str = compact_value_string(expected);
            let obs_str = compact_value_string(observed);
            let display_path = if path.is_empty() {
                "(root)".to_owned()
            } else {
                path
            };
            changes.push(format!("changed: {display_path}: {exp_str} -> {obs_str}"));
        }
    }
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
