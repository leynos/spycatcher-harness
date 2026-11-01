//! Contract tests for the dev-build footprint standard (DF-001 to DF-003).
//!
//! The default development profile (set in `.cargo/config.toml`) must not
//! keep incremental state or debug information, and the pinned nightly must avoid duplicated
//! metadata in `.rlib` files through the native Cargo key rather than a raw `rustc` flag.
//! `nightly-2026-07-31` and later pins use `[unstable] embed-metadata = false`;
//! the older `no-embed-metadata` key must not come back with them.
//!
//! File access goes through `cap_std` directory handles rooted at the crate
//! manifest directory.

use std::error::Error;

use cap_std::{ambient_authority, fs::Dir};

/// The result of a reader, which the tests unwrap.
type Read<T> = Result<T, Box<dyn Error>>;

/// Reads a TOML file relative to the crate manifest directory.
fn read_toml(path: &str) -> Read<toml::Value> {
    let root = Dir::open_ambient_dir(env!("CARGO_MANIFEST_DIR"), ambient_authority())?;
    Ok(toml::from_str(&root.read_to_string(path)?)?)
}

/// Follows a path of table keys through a TOML value.
fn value_at<'a>(root: &'a toml::Value, path: &[&str]) -> Option<&'a toml::Value> {
    path.iter().try_fold(root, |value, key| value.get(key))
}

#[test]
fn the_configuration_avoids_duplicated_rlib_metadata_with_the_native_key() {
    let config = read_toml(".cargo/config.toml").expect("read the Cargo configuration");
    assert_eq!(
        value_at(&config, &["unstable", "embed-metadata"]).and_then(toml::Value::as_bool),
        Some(false),
        "`[unstable] embed-metadata = false` is the native key for this pin"
    );
    assert!(
        value_at(&config, &["unstable", "no-embed-metadata"]).is_none(),
        "`no-embed-metadata` is the key for pins before nightly-2026-07-31"
    );
}

#[test]
fn the_development_profile_keeps_no_incremental_state_or_debug_information() {
    let manifest = read_toml(".cargo/config.toml").expect("read the Cargo configuration");
    assert_eq!(
        value_at(&manifest, &["profile", "dev", "incremental"]).and_then(toml::Value::as_bool),
        Some(false),
        "`[profile.dev] incremental = false` keeps incremental state out of target"
    );
    assert_eq!(
        value_at(&manifest, &["profile", "dev", "debug"]).and_then(toml::Value::as_integer),
        Some(0),
        "`[profile.dev] debug = 0` keeps debug information out of target"
    );
}
