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
/// let loader = FluentLanguageLoader::new("spycatcher-harness", fallback.clone());
/// i18n_embed::select(&loader, &HarnessLocalizations, &[fallback])?;
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(RustEmbed)]
#[folder = "i18n/"]
pub struct HarnessLocalizations;

/// Renders a localised message for a harness error through the caller's
/// language loader.
///
/// The caller owns locale negotiation, fallback policy, and loader
/// lifecycle. If the loader has not been populated with this crate's
/// embedded resources, rendering falls back to the error's existing
/// non-localised [`std::fmt::Display`] text.
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
/// assert_eq!(rendered, "invalid configuration: missing upstream");
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[must_use]
pub fn localize_harness_error(loader: &FluentLanguageLoader, error: &HarnessError) -> String {
    let (id, args) = harness_error_message(error);
    if loader.has(id) {
        let rendered = loader.get_args(id, args.clone());
        strip_fluent_isolation_marks(&rendered, args.values())
    } else {
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
        HarnessError::Io { source } => (IO, HashMap::from([("source", source.to_string())])),
    }
}

fn strip_fluent_isolation_marks<'a>(
    rendered: &str,
    arg_values: impl IntoIterator<Item = &'a String>,
) -> String {
    arg_values
        .into_iter()
        .fold(rendered.to_owned(), |text, value| {
            text.replace(&format!("\u{2068}{value}\u{2069}"), value)
        })
}

#[cfg(test)]
mod tests {
    //! Unit tests for library-owned Fluent resources and injected rendering.

    use super::*;
    use i18n_embed::fluent::FluentLanguageLoader;
    use i18n_embed::unic_langid::LanguageIdentifier;
    use proptest::prelude::*;
    use rstest::{fixture, rstest};

    #[rstest]
    fn localize_harness_error_invalid_config(
        #[from(english_loader)] english_loader_result: Result<
            FluentLanguageLoader,
            Box<dyn std::error::Error>,
        >,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let english_loader = english_loader_result?;
        let error = HarnessError::InvalidConfig {
            message: "missing upstream".to_owned(),
        };
        let actual = localize_harness_error(&english_loader, &error);

        insta::assert_snapshot!(actual, @"invalid configuration: missing upstream");
        Ok(())
    }

    #[rstest]
    fn localize_harness_error_cassette_not_found(
        #[from(english_loader)] english_loader_result: Result<
            FluentLanguageLoader,
            Box<dyn std::error::Error>,
        >,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let english_loader = english_loader_result?;
        let error = HarnessError::CassetteNotFound {
            cassette_name: "session.json".to_owned(),
        };
        let actual = localize_harness_error(&english_loader, &error);

        insta::assert_snapshot!(actual, @"cassette not found: session.json");
        Ok(())
    }

    #[rstest]
    fn localize_harness_error_request_mismatch(
        #[from(english_loader)] english_loader_result: Result<
            FluentLanguageLoader,
            Box<dyn std::error::Error>,
        >,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let english_loader = english_loader_result?;
        let error = HarnessError::RequestMismatch {
            interaction_id: 2,
            expected_hash: "abc".to_owned(),
            observed_hash: "def".to_owned(),
            diff_summary: "method differs".to_owned(),
        };
        let actual = localize_harness_error(&english_loader, &error);

        insta::assert_snapshot!(
            actual,
            @"request mismatch at interaction 2: expected abc, observed def"
        );
        Ok(())
    }

    #[rstest]
    fn localize_harness_error_invalid_cassette(
        #[from(english_loader)] english_loader_result: Result<
            FluentLanguageLoader,
            Box<dyn std::error::Error>,
        >,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let english_loader = english_loader_result?;
        let error = HarnessError::InvalidCassette {
            message: "missing interactions".to_owned(),
        };
        let actual = localize_harness_error(&english_loader, &error);

        insta::assert_snapshot!(actual, @"invalid cassette: missing interactions");
        Ok(())
    }

    #[rstest]
    fn localize_harness_error_unsupported_version(
        #[from(english_loader)] english_loader_result: Result<
            FluentLanguageLoader,
            Box<dyn std::error::Error>,
        >,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let english_loader = english_loader_result?;
        let error = HarnessError::UnsupportedCassetteFormatVersion {
            found: 1,
            supported: 2,
        };
        let actual = localize_harness_error(&english_loader, &error);

        insta::assert_snapshot!(
            actual,
            @"unsupported cassette format version 1; supported version is 2"
        );
        Ok(())
    }

    #[rstest]
    fn localize_harness_error_upstream_failure(
        #[from(english_loader)] english_loader_result: Result<
            FluentLanguageLoader,
            Box<dyn std::error::Error>,
        >,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let english_loader = english_loader_result?;
        let error = HarnessError::UpstreamRequestFailed {
            source: Box::new(std::io::Error::other("timed out")),
        };
        let actual = localize_harness_error(&english_loader, &error);

        insta::assert_snapshot!(actual, @"upstream request failed: timed out");
        Ok(())
    }

    #[rstest]
    fn localize_harness_error_mode_not_yet_implemented(
        #[from(english_loader)] english_loader_result: Result<
            FluentLanguageLoader,
            Box<dyn std::error::Error>,
        >,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let english_loader = english_loader_result?;
        let error = HarnessError::ModeNotYetImplemented {
            mode: "Verify".to_owned(),
        };
        let actual = localize_harness_error(&english_loader, &error);

        insta::assert_snapshot!(actual, @"mode not yet implemented: Verify");
        Ok(())
    }

    #[rstest]
    fn localize_harness_error_io(
        #[from(english_loader)] english_loader_result: Result<
            FluentLanguageLoader,
            Box<dyn std::error::Error>,
        >,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let english_loader = english_loader_result?;
        let error = HarnessError::Io {
            source: std::io::Error::other("disk full"),
        };
        let actual = localize_harness_error(&english_loader, &error);

        insta::assert_snapshot!(actual, @"io failure: disk full");
        Ok(())
    }

    #[rstest]
    fn localize_harness_error_falls_back_when_loader_is_unloaded(
        fallback_language: Result<LanguageIdentifier, Box<dyn std::error::Error>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let loader = FluentLanguageLoader::new("spycatcher-harness", fallback_language?);
        let error = HarnessError::InvalidConfig {
            message: "missing upstream".to_owned(),
        };

        insta::assert_snapshot!(
            localize_harness_error(&loader, &error),
            @"invalid configuration: missing upstream"
        );
        Ok(())
    }

    #[rstest]
    fn localize_harness_error_preserves_intentional_isolation_marks(
        #[from(english_loader)] english_loader_result: Result<
            FluentLanguageLoader,
            Box<dyn std::error::Error>,
        >,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let english_loader = english_loader_result?;
        let error = HarnessError::InvalidConfig {
            message: "\u{2068}already isolated\u{2069}".to_owned(),
        };

        insta::assert_snapshot!(
            localize_harness_error(&english_loader, &error),
            @"invalid configuration: \u{2068}already isolated\u{2069}"
        );
        Ok(())
    }

    #[fixture]
    fn english_loader(
        #[from(fallback_language)] fallback_language_result: Result<
            LanguageIdentifier,
            Box<dyn std::error::Error>,
        >,
    ) -> Result<FluentLanguageLoader, Box<dyn std::error::Error>> {
        let fallback_language = fallback_language_result?;
        let loader = FluentLanguageLoader::new("spycatcher-harness", fallback_language.clone());
        i18n_embed::select(&loader, &HarnessLocalizations, &[fallback_language])?;
        Ok(loader)
    }

    #[fixture]
    fn fallback_language() -> Result<LanguageIdentifier, Box<dyn std::error::Error>> {
        Ok("en-US".parse()?)
    }

    proptest! {
        #[test]
        fn strip_fluent_isolation_marks_removes_only_wrapped_arguments(value in ".*") {
            let rendered = format!("before \u{2068}{value}\u{2069} after");
            let arg_values = [value.clone()];

            prop_assert_eq!(
                strip_fluent_isolation_marks(&rendered, arg_values.iter()),
                format!("before {value} after"),
            );
        }

        #[test]
        fn strip_fluent_isolation_marks_preserves_unmatched_text(
            rendered in ".*",
            value in ".*",
        ) {
            prop_assume!(!rendered.contains(&format!("\u{2068}{value}\u{2069}")));
            let arg_values = [value];

            prop_assert_eq!(
                strip_fluent_isolation_marks(&rendered, arg_values.iter()),
                rendered,
            );
        }
    }
}
