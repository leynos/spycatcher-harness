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

    #[test]
    fn strip_fluent_isolation_marks_removes_multiple_wrapped_arguments(
        a in ".+",
        b in ".+",
    ) {
        prop_assume!(a != b);
        let rendered = format!("first \u{2068}{a}\u{2069} second \u{2068}{b}\u{2069}");
        let arg_values = [a.clone(), b.clone()];

        prop_assert_eq!(
            strip_fluent_isolation_marks(&rendered, arg_values.iter()),
            format!("first {a} second {b}"),
        );
    }

    #[test]
    fn strip_fluent_isolation_marks_is_idempotent(value in ".*") {
        let rendered = format!("before \u{2068}{value}\u{2069} after");
        let arg_values = [value];
        let once = strip_fluent_isolation_marks(&rendered, arg_values.iter());
        let twice = strip_fluent_isolation_marks(&once, arg_values.iter());

        prop_assert_eq!(once, twice);
    }

    #[test]
    fn strip_fluent_isolation_marks_preserves_isolation_chars_inside_argument(
        prefix in ".*",
        suffix in ".*",
        marker in prop_oneof![Just("\u{2068}"), Just("\u{2069}")],
    ) {
        let value = format!("{prefix}{marker}{suffix}");
        let rendered = format!("before \u{2068}{value}\u{2069} after");
        let arg_values = [value.clone()];

        prop_assert_eq!(
            strip_fluent_isolation_marks(&rendered, arg_values.iter()),
            format!("before {value} after"),
        );
    }

    #[test]
    fn strip_fluent_isolation_marks_leaves_empty_argument_list_unchanged(
        rendered in ".*",
    ) {
        prop_assert_eq!(
            strip_fluent_isolation_marks(&rendered, std::iter::empty::<&String>()),
            rendered,
        );
    }
}
