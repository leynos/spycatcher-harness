//! Unit tests for the canonical JSON diff utility.

use std::collections::BTreeMap;

use proptest::prelude::*;
use rstest::rstest;
use serde_json::{Value, json};

use super::diff::canonical_diff_summary;

#[rstest]
fn identical_values_produce_empty_summary() {
    let value = json!({"method": "POST", "path": "/api"});
    let diff = canonical_diff_summary(&value, &value);
    assert_eq!(diff, "");
}

#[rstest]
fn added_top_level_key_is_reported() {
    let expected = json!({"method": "POST"});
    let observed = json!({"method": "POST", "extra": "value"});
    let diff = canonical_diff_summary(&expected, &observed);
    assert!(diff.contains("added: extra: \"value\""), "diff: {diff}");
}

#[rstest]
fn removed_top_level_key_is_reported() {
    let expected = json!({"method": "POST", "path": "/api"});
    let observed = json!({"method": "POST"});
    let diff = canonical_diff_summary(&expected, &observed);
    assert!(diff.contains("removed: path"), "diff: {diff}");
}

#[rstest]
fn changed_scalar_value_is_reported_with_both_values() {
    let expected = json!({"method": "POST"});
    let observed = json!({"method": "GET"});
    let diff = canonical_diff_summary(&expected, &observed);
    assert!(diff.contains("changed: method"), "diff: {diff}");
    assert!(diff.contains("\"POST\""), "diff: {diff}");
    assert!(diff.contains("\"GET\""), "diff: {diff}");
}

#[rstest]
fn nested_object_differences_use_dotted_path_notation() {
    let expected = json!({
        "canonical_body": {
            "metadata": {
                "run_id": "old_id"
            }
        }
    });
    let observed = json!({
        "canonical_body": {
            "metadata": {
                "run_id": "new_id"
            }
        }
    });
    let diff = canonical_diff_summary(&expected, &observed);
    assert!(
        diff.contains("changed: canonical_body.metadata.run_id"),
        "diff: {diff}"
    );
    assert!(diff.contains("\"old_id\""), "diff: {diff}");
    assert!(diff.contains("\"new_id\""), "diff: {diff}");
}

#[rstest]
fn type_mismatch_is_reported() {
    let expected = json!({"value": 42});
    let observed = json!({"value": "forty-two"});
    let diff = canonical_diff_summary(&expected, &observed);
    assert!(diff.contains("changed: value"), "diff: {diff}");
    assert!(diff.contains("42"), "diff: {diff}");
    assert!(diff.contains("\"forty-two\""), "diff: {diff}");
}

#[rstest]
fn root_level_scalar_mismatch_reports_root_path() {
    let expected = json!(42);
    let observed = json!(99);
    let diff = canonical_diff_summary(&expected, &observed);
    assert!(diff.contains("changed: (root)"), "diff: {diff}");
    assert!(diff.contains("42"), "diff: {diff}");
    assert!(diff.contains("99"), "diff: {diff}");
}

#[rstest]
fn array_element_added_is_reported() {
    let expected = json!({"items": [1, 2]});
    let observed = json!({"items": [1, 2, 3]});
    let diff = canonical_diff_summary(&expected, &observed);
    assert!(diff.contains("added: items[2]"), "diff: {diff}");
}

#[rstest]
fn array_element_removed_is_reported() {
    let expected = json!({"items": [1, 2, 3]});
    let observed = json!({"items": [1, 2]});
    let diff = canonical_diff_summary(&expected, &observed);
    assert!(diff.contains("removed: items[2]"), "diff: {diff}");
}

#[rstest]
fn array_element_changed_is_reported() {
    let expected = json!({"items": [1, 2, 3]});
    let observed = json!({"items": [1, 99, 3]});
    let diff = canonical_diff_summary(&expected, &observed);
    assert!(diff.contains("changed: items[1]"), "diff: {diff}");
    assert!(diff.contains("2 -> 99"), "diff: {diff}");
}

#[rstest]
fn nested_object_added_key_is_reported() {
    let expected = json!({
        "canonical_body": {
            "existing": "value"
        }
    });
    let observed = json!({
        "canonical_body": {
            "existing": "value",
            "new_field": "new_value"
        }
    });
    let diff = canonical_diff_summary(&expected, &observed);
    assert!(
        diff.contains("added: canonical_body.new_field"),
        "diff: {diff}"
    );
}

#[rstest]
fn nested_object_removed_key_is_reported() {
    let expected = json!({
        "canonical_body": {
            "metadata": {
                "run_id": "abc"
            }
        }
    });
    let observed = json!({
        "canonical_body": {
            "metadata": {}
        }
    });
    let diff = canonical_diff_summary(&expected, &observed);
    assert!(
        diff.contains("removed: canonical_body.metadata.run_id"),
        "diff: {diff}"
    );
}

#[rstest]
fn complex_nested_structure_with_multiple_differences() {
    let expected = json!({
        "method": "POST",
        "canonical_query": "a=1&b=2",
        "canonical_body": {
            "messages": [
                {"role": "user", "content": "hello"}
            ]
        }
    });
    let observed = json!({
        "method": "GET",
        "canonical_query": "a=1&c=3",
        "canonical_body": {
            "messages": [
                {"role": "assistant", "content": "hello"}
            ],
            "extra_field": "value"
        }
    });
    let diff = canonical_diff_summary(&expected, &observed);
    assert!(diff.contains("changed: method"), "diff: {diff}");
    assert!(diff.contains("changed: canonical_query"), "diff: {diff}");
    assert!(
        diff.contains("changed: canonical_body.messages[0].role"),
        "diff: {diff}"
    );
    assert!(
        diff.contains("added: canonical_body.extra_field"),
        "diff: {diff}"
    );
}

// ── snapshot tests ──────────────────────────────────────────────────────────

#[test]
fn snapshot_changed_scalar() {
    let expected = json!({"method": "POST"});
    let observed = json!({"method": "GET"});
    let diff = canonical_diff_summary(&expected, &observed);
    insta::assert_snapshot!(diff, @r#"changed: method: "POST" -> "GET""#);
}

#[test]
fn snapshot_added_and_removed_keys() {
    let expected = json!({"alpha": 1, "beta": 2});
    let observed = json!({"beta": 2, "gamma": 3});
    let diff = canonical_diff_summary(&expected, &observed);
    insta::assert_snapshot!(diff, @r"
    removed: alpha
    added: gamma: 3
    ");
}

#[test]
fn snapshot_complex_nested_diff() {
    let expected = json!({
        "method": "POST",
        "canonical_query": "a=1&b=2",
        "canonical_body": {
            "messages": [
                {"role": "user", "content": "hello"}
            ]
        }
    });
    let observed = json!({
        "method": "GET",
        "canonical_query": "a=1&c=3",
        "canonical_body": {
            "messages": [
                {"role": "assistant", "content": "hello"}
            ],
            "extra_field": "value"
        }
    });
    let diff = canonical_diff_summary(&expected, &observed);
    insta::assert_snapshot!(diff, @r#"
    added: canonical_body.extra_field: "value"
    changed: canonical_body.messages[0].role: "user" -> "assistant"
    changed: canonical_query: "a=1&b=2" -> "a=1&c=3"
    changed: method: "POST" -> "GET"
    "#);
}

#[test]
fn snapshot_array_length_mismatch() {
    let expected = json!({"items": [1, 2, 3, 4]});
    let observed = json!({"items": [1, 99]});
    let diff = canonical_diff_summary(&expected, &observed);
    insta::assert_snapshot!(diff, @r"
    changed: items[1]: 2 -> 99
    removed: items[2]
    removed: items[3]
    ");
}

#[test]
fn snapshot_type_mismatch_object_to_scalar() {
    let expected = json!({"value": {"nested": true}});
    let observed = json!({"value": 42});
    let diff = canonical_diff_summary(&expected, &observed);
    insta::assert_snapshot!(diff, @"changed: value: {...} -> 42");
}

#[test]
fn snapshot_empty_vs_populated_object() {
    let expected = json!({});
    let observed = json!({"a": 1, "b": "two", "c": null});
    let diff = canonical_diff_summary(&expected, &observed);
    insta::assert_snapshot!(diff, @r#"
    added: a: 1
    added: b: "two"
    added: c: null
    "#);
}

#[test]
fn snapshot_deeply_nested_change() {
    let expected = json!({"a": {"b": {"c": {"d": "old"}}}});
    let observed = json!({"a": {"b": {"c": {"d": "new"}}}});
    let diff = canonical_diff_summary(&expected, &observed);
    insta::assert_snapshot!(diff, @r#"changed: a.b.c.d: "old" -> "new""#);
}

// ── property-based tests ────────────────────────────────────────────────────

/// Strategy producing small JSON objects with string keys and simple values.
fn arb_json_object() -> impl Strategy<Value = Value> {
    let leaf = prop_oneof![
        any::<bool>().prop_map(Value::Bool),
        (-1000i64..1000i64).prop_map(|n| Value::Number(n.into())),
        "[a-z]{1,8}".prop_map(Value::String),
        Just(Value::Null),
    ];

    // Build a flat object with 0..6 keys drawn from a small alphabet
    prop::collection::btree_map("[a-z]{1,4}", leaf, 0..6).prop_map(|map| {
        let serde_map: serde_json::Map<String, Value> = map.into_iter().collect();
        Value::Object(serde_map)
    })
}

proptest! {
    #[test]
    fn identical_values_always_produce_empty_diff(obj in arb_json_object()) {
        let diff = canonical_diff_summary(&obj, &obj);
        prop_assert_eq!(diff, "");
    }

    #[test]
    fn diff_is_deterministic_across_invocations(
        expected in arb_json_object(),
        observed in arb_json_object(),
    ) {
        let diff_a = canonical_diff_summary(&expected, &observed);
        let diff_b = canonical_diff_summary(&expected, &observed);
        prop_assert_eq!(diff_a, diff_b, "diff output should be deterministic");
    }

    #[test]
    fn every_differing_key_is_mentioned(
        expected in arb_json_object(),
        observed in arb_json_object(),
    ) {
        let diff = canonical_diff_summary(&expected, &observed);

        let empty = serde_json::Map::new();
        let exp_map = expected.as_object().unwrap_or(&empty).clone();
        let obs_map = observed.as_object().unwrap_or(&empty).clone();

        // Collect all keys with actual differences
        let mut all_keys = BTreeMap::new();
        for (k, v) in &exp_map {
            all_keys.entry(k.clone()).or_insert((Some(v), None)).0 = Some(v);
        }
        for (k, v) in &obs_map {
            all_keys.entry(k.clone()).or_insert((None, None)).1 = Some(v);
        }

        for (key, (exp_val, obs_val)) in &all_keys {
            match (exp_val, obs_val) {
                (Some(e), Some(o)) if e == o => {
                    // No diff expected
                }
                _ => {
                    // Key should be mentioned in the diff output
                    prop_assert!(
                        diff.contains(key.as_str()),
                        "key {:?} differs but not mentioned in diff:\n{}",
                        key,
                        diff
                    );
                }
            }
        }
    }

    #[test]
    fn diff_key_ordering_does_not_affect_output(
        base in arb_json_object(),
        observed in arb_json_object(),
    ) {
        // Rebuild expected with reversed key insertion order to verify
        // the diff engine sorts keys internally
        let empty = serde_json::Map::new();
        let exp_map = base.as_object().unwrap_or(&empty).clone();
        let reversed: serde_json::Map<String, Value> = exp_map.into_iter().rev().collect();
        let reversed_obj = Value::Object(reversed);

        let diff_original = canonical_diff_summary(&base, &observed);
        let diff_reversed = canonical_diff_summary(&reversed_obj, &observed);
        prop_assert_eq!(
            diff_original,
            diff_reversed,
            "key insertion order must not affect diff output"
        );
    }
}
