//! Jail-backed configuration loading and locale strategies for layering tests.

use std::cell::RefCell;

use eyre::{Result, eyre};
use ortho_config::figment;
use proptest::prop_oneof;
use proptest::strategy::{Just, Strategy};

use spycatcher_harness::HarnessConfig;
use spycatcher_harness::cli::load_subcommand_config_from_iter;

/// Loads a [`HarnessConfig`] under `figment::Jail` using `argv`, optional
/// `config_file` contents, and `env_vars`, returning the loaded config or
/// conversion error.
#[expect(
    clippy::result_large_err,
    reason = "figment::Jail callback requires figment::error::Result"
)]
pub(super) fn load_with_jail(
    argv: &[&str],
    config_file: Option<&str>,
    env_vars: &[(&str, &str)],
) -> Result<HarnessConfig> {
    let loaded = RefCell::new(None);

    figment::Jail::try_with(|jail| {
        if let Some(content) = config_file {
            jail.create_file(".spycatcher_harness.toml", content)?;
        }
        for (key, value) in env_vars {
            jail.set_env(key, value);
        }
        let cfg = load_subcommand_config_from_iter(argv)
            .map_err(|error| figment::Error::from(error.to_string()))?;
        loaded.replace(Some(cfg));
        Ok(())
    })
    .map_err(|error| eyre!(error.to_string()))?;

    loaded
        .into_inner()
        .ok_or_else(|| eyre!("configuration load did not execute"))
}

pub(super) const REPLAY_FILE_CONFIG: &str = "[cmds.replay]\ncassette_name = \"from_file\"\n";
pub(super) const REPLAY_LOCALIZATION_FILE_CONFIG: &str = concat!(
    "[cmds.replay.localization]\n",
    "locale = \"en-GB\"\n",
    "fallback_locale = \"en-US\"\n",
);

/// Generates lowercase two- or three-letter language subtags as a strategy.
fn language_subtag() -> impl Strategy<Value = String> {
    proptest::collection::vec(b'a'..=b'z', 2..=3)
        .prop_map(|bytes| bytes.into_iter().map(char::from).collect())
}

/// Generates uppercase two-letter region subtags as a strategy.
fn region_subtag() -> impl Strategy<Value = String> {
    proptest::collection::vec(b'A'..=b'Z', 2)
        .prop_map(|bytes| bytes.into_iter().map(char::from).collect())
}

/// Generates titlecase four-letter script subtags as a strategy.
fn script_subtag() -> impl Strategy<Value = String> {
    (b'A'..=b'Z', proptest::collection::vec(b'a'..=b'z', 3))
        .prop_map(|(first, rest)| std::iter::once(first).chain(rest).map(char::from).collect())
}

/// Generates lowercase five- to eight-letter variant subtags as a strategy.
fn variant_subtag() -> impl Strategy<Value = String> {
    prop_oneof![
        Just(String::from("valencia")),
        proptest::collection::vec(b'a'..=b'z', 5..=8)
            .prop_map(|bytes| bytes.into_iter().map(char::from).collect()),
    ]
}

/// Generates valid locale text such as `xx`, `xx-Xxxx`, `xx-YY`, or
/// `xx-valencia` as a strategy.
pub(super) fn valid_locale_text() -> impl Strategy<Value = String> {
    (
        language_subtag(),
        proptest::option::of(script_subtag()),
        proptest::option::of(region_subtag()),
        proptest::option::of(variant_subtag()),
    )
        .prop_map(|(language, script, region, variant)| {
            let mut locale = language;
            for subtag in [script, region, variant].into_iter().flatten() {
                locale.push('-');
                locale.push_str(&subtag);
            }
            locale
        })
}

pub(super) fn optional_locale_text() -> impl Strategy<Value = Option<String>> {
    proptest::option::of(valid_locale_text())
}
