//! Embedded Fluent Translation List (FTL) resources and message IDs for the
//! Spycatcher harness.
//!
//! This module contains library-owned localization assets and rendering APIs
//! that accept an application-provided language loader.
//! See `docs/localizable-rust-libraries-with-fluent.md` for the
//! localization architecture.

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
/// let loader = FluentLanguageLoader::new("spycatcher-harness", fallback.clone());
/// i18n_embed::select(&loader, &HarnessLocalizations, &[fallback])?;
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
/// non-localized [`std::fmt::Display`] text and emits a `debug`-level log
/// entry so callers can detect that fallback through their log subscriber.
/// Fluent's formatted output is returned unchanged, including bidirectional
/// isolation marks around placeables.
///
/// # Security considerations
///
/// Fluent arguments are passed as named values and are not re-parsed as FTL,
/// so user-provided strings cannot escape the template or execute arbitrary
/// selectors. The `HarnessError::Io` variant includes the underlying
/// [`std::io::Error`] text for diagnostic value; callers should treat that
/// text as potentially user-controlled and capable of carrying sensitive path
/// information.
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
/// let loader = FluentLanguageLoader::new("spycatcher-harness", fallback.clone());
/// i18n_embed::select(&loader, &HarnessLocalizations, &[fallback])?;
///
/// let error = HarnessError::InvalidConfig {
///     message: "missing upstream".to_owned(),
/// };
/// let rendered = localize_harness_error(&loader, &error);
///
/// assert_eq!(rendered, "invalid configuration: \u{2068}missing upstream\u{2069}");
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[must_use]
pub fn localize_harness_error(loader: &FluentLanguageLoader, error: &HarnessError) -> String {
    let (id, args) = harness_error_message(error);
    if loader.has(id) {
        loader.get_args(id, args)
    } else {
        log::debug!("Fluent message '{id}' not available in loader; falling back to Display text");
        error.to_string()
    }
}

fn harness_error_message(error: &HarnessError) -> (&'static str, HashMap<&'static str, String>) {
    match error {
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
            diff_summary,
        } => (
            REQUEST_MISMATCH,
            HashMap::from([
                ("interaction_id", interaction_id.to_string()),
                ("expected_hash", expected_hash.clone()),
                ("observed_hash", observed_hash.clone()),
                ("diff_summary", diff_summary.clone()),
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
        HarnessError::Io { source } => (IO, HashMap::from([("source", source.to_string())])),
    }
}

#[cfg(test)]
#[path = "i18n_tests.rs"]
mod tests;
