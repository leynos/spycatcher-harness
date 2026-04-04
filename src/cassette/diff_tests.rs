//! Unit tests for the canonical JSON diff utility.

use super::diff::canonical_diff_summary;
use rstest::rstest;
use serde_json::json;

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
