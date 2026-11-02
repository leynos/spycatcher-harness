//! Process-level coverage for the normal CLI configuration loader.
//!
//! These tests isolate each binary invocation and verify that its
//! process-backed file, environment, and explicit CLI inputs retain their
//! precedence without mutating the test process environment.

use std::{io::Write as _, process::Command};

use cap_std::{
    ambient_authority,
    fs_utf8::{Dir, camino::Utf8Path},
};
use rstest::rstest;

#[path = "common/coverage_profile.rs"]
mod coverage_profile;

const RECORD_LOCALE_ENV: &str = "SPYCATCHER_HARNESS_CMDS_RECORD_LOCALIZATION__LOCALE";

/// Proves the process-backed loader applies CLI over environment over file
/// precedence for localization settings.
///
/// The child runs with an isolated home and XDG roots, so only the values
/// written by this case can influence the result.
#[rstest]
#[case::defaults(None, None, None, "upstream configuration is required")]
#[case::file_over_default(Some("from_file"), None, None, "from_file")]
#[case::environment_over_file(Some("from_file"), Some("from_env"), None, "from_env")]
#[case::cli_over_environment(
    Some("from_file"),
    Some("from_env"),
    Some("en-GB"),
    "upstream configuration is required"
)]
fn process_backed_cli_preserves_configuration_precedence(
    #[case] file_locale: Option<&str>,
    #[case] environment_locale: Option<&str>,
    #[case] cli_locale: Option<&str>,
    #[case] expected_error: &str,
) {
    let directory = tempfile::tempdir().expect("configuration directory should be created");
    let utf8_path = Utf8Path::from_path(directory.path())
        .expect("temporary configuration path should be UTF-8");
    let config_dir = Dir::open_ambient_dir(utf8_path, ambient_authority())
        .expect("configuration directory should open");
    if let Some(locale) = file_locale {
        let mut file = config_dir
            .create(".spycatcher_harness.toml")
            .expect("configuration file should be created");
        write!(file, "[cmds.record.localization]\nlocale = \"{locale}\"\n")
            .expect("configuration file should be written");
    }

    let config_root = directory
        .path()
        .to_str()
        .expect("temporary configuration path should be UTF-8");
    let mut command = Command::new(env!("CARGO_BIN_EXE_spycatcher-harness"));
    command
        .env_clear()
        .current_dir(directory.path())
        .env("HOME", directory.path())
        .env("XDG_CONFIG_HOME", config_root)
        .env("XDG_CONFIG_DIRS", config_root)
        .arg("record");
    if let Some(locale) = environment_locale {
        command.env(RECORD_LOCALE_ENV, locale);
    }
    if let Some(locale) = cli_locale {
        command.args(["--locale", locale]);
    }
    coverage_profile::preserve_coverage_profile(&mut command);
    let output = command.output().expect("CLI process should execute");

    assert_eq!(output.status.code(), Some(1), "startup should fail cleanly");
    assert!(output.stdout.is_empty(), "startup errors belong on stderr");
    let stderr = String::from_utf8(output.stderr).expect("stderr should be UTF-8");
    assert!(
        stderr.contains(expected_error),
        "stderr should contain {expected_error:?}: {stderr}"
    );
    for lower_priority_locale in [file_locale, environment_locale].into_iter().flatten() {
        if Some(lower_priority_locale) != cli_locale && lower_priority_locale != expected_error {
            assert!(
                !stderr.contains(lower_priority_locale),
                "lower-priority locale {lower_priority_locale:?} leaked into stderr: {stderr}"
            );
        }
    }
}
