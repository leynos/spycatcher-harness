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
