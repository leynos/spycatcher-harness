//! Shared file fixtures and locale strategies for CLI layering tests.

#[path = "source_loader.rs"]
mod source_loader;

pub(super) use source_loader::{load_from_base, load_with_sources};

use proptest::prop_oneof;
use proptest::strategy::{Just, Strategy};

/// Replay cassette-name file fixture for checking overrides of `from_file`.
pub(super) const REPLAY_FILE_CONFIG: &str = "[cmds.replay]\ncassette_name = \"from_file\"\n";
/// Replay locale file fixture with `en-GB` primary and `en-US` fallback values.
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

/// Builds a strategy for absent or valid locale text in layering scenarios.
/// For example, `optional_locale_text()` can generate `None` or
/// `Some("en-GB".to_owned())`; it returns a strategy, not a selected locale.
pub(super) fn optional_locale_text() -> impl Strategy<Value = Option<String>> {
    proptest::option::of(valid_locale_text())
}
