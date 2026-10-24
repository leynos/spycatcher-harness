//! Tests for accepted configuration shapes under command-specific namespaces.

use rstest::rstest;

use super::{ModeSnapshot, load_with_child};

#[rstest]
fn record_supports_cmds_namespace_for_nested_upstream_values() {
    let config = concat!(
        "[cmds.record]\n",
        "cassette_name = \"cassette_a\"\n",
        "[cmds.record.upstream]\n",
        "kind = \"openrouter\"\n",
        "base_url = \"https://example.invalid/api\"\n",
        "api_key_env = \"TEST_API_KEY\"\n"
    );

    let loaded = load_with_child(&["spycatcher-harness", "record"], Some(config), &[])
        .expect("record config should load");
    let upstream = loaded
        .upstream
        .expect("record config should contain upstream values");
    assert_eq!(loaded.cassette_name, "cassette_a");
    assert_eq!(loaded.mode, ModeSnapshot::Record);
    assert_eq!(upstream.base_url.as_str(), "https://example.invalid/api");
    assert_eq!(upstream.api_key_env, "TEST_API_KEY");
}

#[rstest]
fn verify_supports_cmds_namespace_overrides() {
    let config = "[cmds.verify]\ncassette_name = \"verify_cassette\"\n";
    let loaded = load_with_child(&["spycatcher-harness", "verify"], Some(config), &[])
        .expect("verify config should load");
    assert_eq!(loaded.cassette_name, "verify_cassette");
    assert_eq!(loaded.mode, ModeSnapshot::Verify);
}

#[rstest]
fn invalid_values_fail_loading() {
    let error = load_with_child(
        &["spycatcher-harness", "replay"],
        None,
        &[("SPYCATCHER_HARNESS_CMDS_REPLAY_LISTEN", "not-an-address")],
    )
    .expect_err("invalid listen should fail loading");
    let message = error.clone();
    assert!(
        (message.contains("invalid") && message.contains("address"))
            || (message.contains("socket") && message.contains("parse")),
        "expected error about invalid listen address, got: {message}",
    );
}
