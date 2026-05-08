//! Embedded Fluent Translation List (FTL) resources and message IDs for the
//! Spycatcher harness.
//!
//! This module will contain library-owned localisation assets and
//! rendering APIs that accept an application-provided language loader.
//! See `docs/localizable-rust-libraries-with-fluent.md` for the
//! localisation architecture.

use std::collections::HashMap;

use i18n_embed::fluent::FluentLanguageLoader;
use rust_embed::RustEmbed;

use crate::HarnessError;

const INVALID_CONFIG: &str = "harness-error-invalid-config";
const CASSETTE_NOT_FOUND: &str = "harness-error-cassette-not-found";
const REQUEST_MISMATCH: &str = "harness-error-request-mismatch";
const INVALID_CASSETTE: &str = "harness-error-invalid-cassette";
const UNSUPPORTED_CASSETTE_FORMAT_VERSION: &str =
    "harness-error-unsupported-cassette-format-version";
const UPSTREAM_REQUEST_FAILED: &str = "harness-error-upstream-request-failed";
const MODE_NOT_YET_IMPLEMENTED: &str = "harness-error-mode-not-yet-implemented";
const IO: &str = "harness-error-io";

/// Embedded Fluent resources owned by the harness library.
///
/// Applications load these assets into their own
/// [`FluentLanguageLoader`] before calling the rendering helpers in
/// this module. The library exposes the resources, but does not create
/// or store a process-global loader.
///
/// # Examples
///
/// ```rust
/// use i18n_embed::fluent::FluentLanguageLoader;
/// use spycatcher_harness::i18n::HarnessLocalizations;
///
/// let fallback = "en-US"
///     .parse::<i18n_embed::unic_langid::LanguageIdentifier>()?;
/// let loader = FluentLanguageLoader::new("spycatcher-harness", fallback);
/// i18n_embed::select(&loader, &HarnessLocalizations, &loader.current_languages())?;
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(RustEmbed)]
#[folder = "i18n/"]
pub struct HarnessLocalizations;

/// Renders a localized message for a harness error through the caller's
/// language loader.
///
/// The caller owns locale negotiation, fallback policy, and loader
/// lifecycle. If the loader has not been populated with this crate's
/// embedded resources, rendering falls back to the error's existing
/// non-localized [`std::fmt::Display`] text.
///
/// # Examples
///
/// ```rust
/// use i18n_embed::fluent::FluentLanguageLoader;
/// use spycatcher_harness::i18n::{localize_harness_error, HarnessLocalizations};
/// use spycatcher_harness::HarnessError;
///
/// let fallback = "en-US"
///     .parse::<i18n_embed::unic_langid::LanguageIdentifier>()?;
/// let loader = FluentLanguageLoader::new("spycatcher-harness", fallback);
/// i18n_embed::select(&loader, &HarnessLocalizations, &loader.current_languages())?;
///
/// let error = HarnessError::InvalidConfig {
///     message: "missing upstream".to_owned(),
/// };
/// let rendered = localize_harness_error(&loader, &error);
///
/// assert_eq!(rendered, "invalid configuration: missing upstream");
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[must_use]
pub fn localize_harness_error(loader: &FluentLanguageLoader, error: &HarnessError) -> String {
    let message = harness_error_message(error);
    let rendered = loader.get_args(message.id, message.args);
    if is_missing_localization(&rendered, message.id) {
        error.to_string()
    } else {
        strip_isolation_marks(&rendered)
    }
}

struct LocalizedMessage<'a> {
    id: &'static str,
    args: HashMap<&'static str, String>,
    _error: &'a HarnessError,
}

fn harness_error_message(error: &HarnessError) -> LocalizedMessage<'_> {
    let (id, args) = match error {
        HarnessError::InvalidConfig { message } => (
            INVALID_CONFIG,
            HashMap::from([("message", message.clone())]),
        ),
        HarnessError::CassetteNotFound { cassette_name } => (
            CASSETTE_NOT_FOUND,
            HashMap::from([("cassette_name", cassette_name.clone())]),
        ),
        HarnessError::RequestMismatch {
            interaction_id,
            expected_hash,
            observed_hash,
            ..
        } => (
            REQUEST_MISMATCH,
            HashMap::from([
                ("interaction_id", interaction_id.to_string()),
                ("expected_hash", expected_hash.clone()),
                ("observed_hash", observed_hash.clone()),
            ]),
        ),
        HarnessError::InvalidCassette { message } => (
            INVALID_CASSETTE,
            HashMap::from([("message", message.clone())]),
        ),
        HarnessError::UnsupportedCassetteFormatVersion { found, supported } => (
            UNSUPPORTED_CASSETTE_FORMAT_VERSION,
            HashMap::from([
                ("found", found.to_string()),
                ("supported", supported.to_string()),
            ]),
        ),
        HarnessError::UpstreamRequestFailed { source } => (
            UPSTREAM_REQUEST_FAILED,
            HashMap::from([("source", source.to_string())]),
        ),
        HarnessError::ModeNotYetImplemented { mode } => (
            MODE_NOT_YET_IMPLEMENTED,
            HashMap::from([("mode", mode.clone())]),
        ),
        HarnessError::Io { .. } => (IO, HashMap::new()),
    };
    LocalizedMessage {
        id,
        args,
        _error: error,
    }
}

fn is_missing_localization(rendered: &str, message_id: &str) -> bool {
    rendered == format!("No localization for id: \"{message_id}\"")
}

fn strip_isolation_marks(rendered: &str) -> String {
    rendered
        .chars()
        .filter(|character| !matches!(character, '\u{2068}' | '\u{2069}'))
        .collect()
}

#[cfg(test)]
mod tests {
    //! Unit tests for library-owned Fluent resources and injected rendering.

    use super::*;
    use i18n_embed::fluent::FluentLanguageLoader;
    use rstest::rstest;

    #[rstest]
    #[case::invalid_config(
        HarnessError::InvalidConfig {
            message: "missing upstream".to_owned(),
        },
        "invalid configuration: missing upstream",
    )]
    #[case::cassette_not_found(
        HarnessError::CassetteNotFound {
            cassette_name: "session.json".to_owned(),
        },
        "cassette not found: session.json",
    )]
    #[case::request_mismatch(
        HarnessError::RequestMismatch {
            interaction_id: 2,
            expected_hash: "abc".to_owned(),
            observed_hash: "def".to_owned(),
            diff_summary: "method differs".to_owned(),
        },
        "request mismatch at interaction 2: expected abc, observed def",
    )]
    #[case::invalid_cassette(
        HarnessError::InvalidCassette {
            message: "missing interactions".to_owned(),
        },
        "invalid cassette: missing interactions",
    )]
    #[case::unsupported_version(
        HarnessError::UnsupportedCassetteFormatVersion {
            found: 1,
            supported: 2,
        },
        "unsupported cassette format version 1; supported version is 2",
    )]
    #[case::upstream_failure(
        HarnessError::UpstreamRequestFailed {
            source: Box::new(std::io::Error::other("timed out")),
        },
        "upstream request failed: timed out",
    )]
    #[case::mode_not_yet_implemented(
        HarnessError::ModeNotYetImplemented {
            mode: "Verify".to_owned(),
        },
        "mode not yet implemented: Verify",
    )]
    #[case::io(
        HarnessError::Io {
            source: std::io::Error::other("disk full"),
        },
        "io failure",
    )]
    fn localize_harness_error_renders_embedded_message(
        #[case] error: HarnessError,
        #[case] expected: &str,
    ) {
        let loader =
            english_loader().expect("English harness localization resources should load for tests");

        assert_eq!(localize_harness_error(&loader, &error), expected);
    }

    #[rstest]
    fn localize_harness_error_falls_back_when_loader_is_unloaded() {
        let fallback =
            fallback_language().expect("English fallback language identifier should parse");
        let loader = FluentLanguageLoader::new("spycatcher-harness", fallback);
        let error = HarnessError::InvalidConfig {
            message: "missing upstream".to_owned(),
        };

        assert_eq!(localize_harness_error(&loader, &error), error.to_string());
    }

    fn english_loader() -> Result<FluentLanguageLoader, Box<dyn std::error::Error>> {
        let loader = FluentLanguageLoader::new("spycatcher-harness", fallback_language()?);
        i18n_embed::select(&loader, &HarnessLocalizations, &loader.current_languages())?;
        Ok(loader)
    }

    fn fallback_language()
    -> Result<i18n_embed::unic_langid::LanguageIdentifier, Box<dyn std::error::Error>> {
        Ok("en-US".parse()?)
    }
}
