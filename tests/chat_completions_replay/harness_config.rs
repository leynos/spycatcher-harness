//! Harness configuration support shared by chat replay and stream BDD steps.
//!
//! Keep cassette-path decomposition and mode-specific defaults together so both
//! step modules construct the same record and replay configurations.

use std::error::Error;
use std::net::SocketAddr;

use url::Url;

use spycatcher_harness::HarnessConfig;
use spycatcher_harness::config::{ListenAddr, Mode, UpstreamConfig, UpstreamKind};

/// Builds a record-mode configuration using the supplied cassette and upstream.
/// For example, `make_record_config(&"cassettes/demo.json".into(), upstream)`
/// uses `cassettes` as the directory and `demo.json` as the cassette name.
pub(super) fn make_record_config(
    cassette_path: &camino::Utf8PathBuf,
    upstream: UpstreamConfig,
) -> Result<HarnessConfig, Box<dyn Error>> {
    let (cassette_dir, cassette_name) = split_cassette_path(cassette_path)?;

    Ok(HarnessConfig {
        listen: ListenAddr::from(SocketAddr::from(([127, 0, 0, 1], 0))),
        mode: Mode::Record,
        cassette_dir,
        cassette_name,
        upstream: Some(upstream),
        ..HarnessConfig::default()
    })
}

/// Builds a replay-mode configuration with an intentionally unused upstream.
/// For example, `make_replay_config(&"cassettes/demo.json".into())` selects that
/// cassette in replay mode without requiring a real upstream credential.
pub(super) fn make_replay_config(
    cassette_path: &camino::Utf8PathBuf,
) -> Result<HarnessConfig, Box<dyn Error>> {
    let (cassette_dir, cassette_name) = split_cassette_path(cassette_path)?;
    let base_url = Url::parse("http://127.0.0.1:1/api/v1")
        .map_err(|error| std::io::Error::other(format!("test fixture URL is invalid: {error}")))?;

    Ok(HarnessConfig {
        listen: ListenAddr::from(SocketAddr::from(([127, 0, 0, 1], 0))),
        mode: Mode::Replay,
        cassette_dir,
        cassette_name,
        upstream: Some(UpstreamConfig {
            kind: UpstreamKind::OpenRouter,
            base_url,
            api_key_env: "SPYCATCHER_REPLAY_SHOULD_NOT_READ".to_owned(),
            extra_headers: std::collections::BTreeMap::new(),
        }),
        ..HarnessConfig::default()
    })
}

/// Separates a cassette path into its parent directory and file name.
/// For example, `split_cassette_path(&"cassettes/demo.json".into())` returns
/// `cassettes` and `demo.json`; a missing parent or file name returns an error.
fn split_cassette_path(
    cassette_path: &camino::Utf8PathBuf,
) -> Result<(camino::Utf8PathBuf, String), Box<dyn Error>> {
    let cassette_name = cassette_path
        .file_name()
        .ok_or_else(|| std::io::Error::other("cassette path should contain a file name"))?
        .to_owned();
    let cassette_dir = cassette_path
        .parent()
        .ok_or_else(|| std::io::Error::other("cassette path should contain a parent directory"))?
        .to_path_buf();
    Ok((cassette_dir, cassette_name))
}

#[cfg(test)]
mod tests {
    //! Lock cassette decomposition and mode-specific BDD configuration contracts.

    use std::collections::BTreeMap;

    use rstest::{fixture, rstest};

    use super::*;

    /// Supplies a distinct upstream so record configuration cannot default it.
    /// For example, the fixture's custom URL and header survive record setup.
    #[fixture]
    fn supplied_upstream() -> Result<UpstreamConfig, Box<dyn Error>> {
        Ok(UpstreamConfig {
            kind: UpstreamKind::OpenRouter,
            base_url: Url::parse("https://example.invalid/custom/v2")?,
            api_key_env: "BDD_SUPPLIED_API_KEY".to_owned(),
            extra_headers: BTreeMap::from([("x-bdd-token".to_owned(), "retained".to_owned())]),
        })
    }

    /// Accepts nested paths and the existing empty-parent semantics of bare names.
    #[rstest]
    #[case::nested("cassettes/demo.json", "cassettes")]
    #[case::bare_name("demo.json", "")]
    fn splits_cassette_paths(
        #[case] path: &str,
        #[case] expected_directory: &str,
    ) -> Result<(), Box<dyn Error>> {
        let (directory, name) = split_cassette_path(&path.into())?;
        if (directory.as_str(), name.as_str()) != (expected_directory, "demo.json") {
            return Err(format!(
                "cassette path: expected ({expected_directory:?}, \"demo.json\"), got ({directory:?}, {name:?})"
            ).into());
        }
        Ok(())
    }

    /// Rejects paths without a filename before the defensive parent check.
    #[rstest]
    #[case::root("/")]
    #[case::empty("")]
    fn rejects_paths_without_file_names(#[case] path: &str) -> Result<(), Box<dyn Error>> {
        let error = split_cassette_path(&path.into())
            .err()
            .ok_or("path should be rejected")?;
        let diagnostic = error.to_string();
        let expected_diagnostic = "cassette path should contain a file name";
        if diagnostic != expected_diagnostic {
            return Err(format!(
                "filename diagnostic: expected {expected_diagnostic:?}, got {diagnostic:?}"
            )
            .into());
        }
        Ok(())
    }

    /// Checks the shared nested-path and ephemeral loopback-listener contract.
    /// For example, `cassettes/demo.json` on loopback port zero returns `Ok(())`;
    /// a different path or listener returns its expected and actual values.
    fn check_path_and_listener(config: &HarnessConfig) -> Result<(), Box<dyn Error>> {
        let actual_path = (config.cassette_dir.as_str(), config.cassette_name.as_str());
        let expected_path = ("cassettes", "demo.json");
        if actual_path != expected_path {
            return Err(
                format!("cassette path: expected {expected_path:?}, got {actual_path:?}").into(),
            );
        }
        let actual_listener = config.listen.as_socket_addr();
        let expected_listener = SocketAddr::from(([127, 0, 0, 1], 0));
        if actual_listener != expected_listener {
            return Err(format!(
                "listener: expected {expected_listener:?}, got {actual_listener:?}"
            )
            .into());
        }
        Ok(())
    }

    /// Retains every supplied upstream field when constructing record mode.
    #[rstest]
    fn record_config_preserves_supplied_upstream(
        supplied_upstream: Result<UpstreamConfig, Box<dyn Error>>,
    ) -> Result<(), Box<dyn Error>> {
        let expected = supplied_upstream?;
        let config = make_record_config(&"cassettes/demo.json".into(), expected.clone())?;
        if config.mode != Mode::Record {
            return Err(format!("mode: expected Record, got {:?}", config.mode).into());
        }
        check_path_and_listener(&config)?;
        let actual = config
            .upstream
            .ok_or("record configuration should retain its upstream")?;
        if (
            actual.kind,
            &actual.base_url,
            &actual.api_key_env,
            &actual.extra_headers,
        ) != (
            expected.kind,
            &expected.base_url,
            &expected.api_key_env,
            &expected.extra_headers,
        ) {
            return Err(format!("record upstream: expected {expected:?}, got {actual:?}").into());
        }
        Ok(())
    }

    /// Keeps replay's intentionally unused upstream independent of credentials.
    #[test]
    fn replay_config_preserves_unused_upstream() -> Result<(), Box<dyn Error>> {
        let config = make_replay_config(&"cassettes/demo.json".into())?;
        if config.mode != Mode::Replay {
            return Err(format!("mode: expected Replay, got {:?}", config.mode).into());
        }
        check_path_and_listener(&config)?;
        let upstream = config
            .upstream
            .ok_or("replay configuration should contain its unused upstream")?;
        let actual_identity = (
            upstream.kind,
            upstream.base_url.as_str(),
            upstream.api_key_env.as_str(),
        );
        let expected_identity = (
            UpstreamKind::OpenRouter,
            "http://127.0.0.1:1/api/v1",
            "SPYCATCHER_REPLAY_SHOULD_NOT_READ",
        );
        if actual_identity != expected_identity {
            return Err(format!(
                "replay upstream: expected {expected_identity:?}, got {actual_identity:?}"
            )
            .into());
        }
        if !upstream.extra_headers.is_empty() {
            return Err(format!(
                "replay headers: expected empty, got {:?}",
                upstream.extra_headers
            )
            .into());
        }
        Ok(())
    }
}
