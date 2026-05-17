//! Integration tests for harness lifecycle startup and shutdown behaviour.

use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};

use camino::Utf8PathBuf;
use cap_std::{ambient_authority, fs_utf8::Dir};
use rstest::rstest;
use spycatcher_harness::cassette::CassetteFormatVersion;
use spycatcher_harness::config::{self, HarnessConfig};
use spycatcher_harness::{HarnessError, HarnessResult, start_harness};
use uuid::Uuid;

static NEXT_TEST_CASSETTE: AtomicUsize = AtomicUsize::new(1);

#[rstest]
#[tokio::test]
async fn start_harness_with_record_config_succeeds() {
    let cfg = record_config(unique_cassette_name("record-valid"));
    let harness = start_harness(cfg)
        .await
        .expect("startup with valid record config should succeed");
    harness.shutdown().await.expect("shutdown should succeed");
}

#[rstest]
#[tokio::test]
#[case::empty("")]
#[case::traversal("../escape")]
#[case::absolute("/tmp/out")]
async fn start_harness_with_invalid_cassette_name_fails(#[case] cassette_name: &str) {
    let cfg = HarnessConfig {
        cassette_name: cassette_name.to_owned(),
        ..HarnessConfig::default()
    };
    let result = start_harness(cfg).await;
    assert!(matches!(result, Err(HarnessError::InvalidConfig { .. })));
}

#[rstest]
#[tokio::test]
async fn start_harness_record_mode_without_upstream_fails() {
    let cfg = HarnessConfig {
        mode: config::Mode::Record,
        upstream: None,
        ..HarnessConfig::default()
    };
    let result = start_harness(cfg).await;
    assert!(matches!(result, Err(HarnessError::InvalidConfig { .. })));
}

#[rstest]
#[tokio::test]
async fn start_harness_record_mode_with_upstream_succeeds() {
    let cassette_name = unique_cassette_name("record-upstream");
    let cfg = record_config(cassette_name.clone());
    let harness = start_harness(cfg)
        .await
        .expect("startup should succeed with upstream");
    let cassette_path = Utf8PathBuf::from("target/test-harness").join(cassette_name);
    assert!(
        cassette_path.is_file(),
        "record startup should create cassette file"
    );
    harness.shutdown().await.expect("shutdown should succeed");
}

#[rstest]
#[tokio::test]
async fn start_harness_cassette_path_joins_dir_and_name() -> HarnessResult<()> {
    let cassette_name = unique_cassette_name("path-join");
    let cassette_dir = Utf8PathBuf::from("target/test-harness");
    seed_replay_cassette(&cassette_name)?;
    let cfg = HarnessConfig {
        cassette_dir: cassette_dir.clone(),
        cassette_name: cassette_name.clone(),
        mode: config::Mode::Replay,
        ..HarnessConfig::default()
    };
    let harness = start_harness(cfg).await.expect("startup should succeed");
    assert_eq!(harness.cassette_path, cassette_dir.join(cassette_name));
    harness.shutdown().await.expect("shutdown should succeed");
    Ok(())
}

#[rstest]
#[tokio::test]
async fn shutdown_succeeds() {
    let cfg = record_config(unique_cassette_name("shutdown"));
    let harness = start_harness(cfg).await.expect("startup should succeed");
    harness.shutdown().await.expect("shutdown should succeed");
}

#[rstest]
#[tokio::test]
async fn start_harness_returns_bound_loopback_address() {
    let requested = SocketAddr::from(([127, 0, 0, 1], 0));
    let cfg = HarnessConfig {
        listen: requested.into(),
        ..record_config(unique_cassette_name("listen"))
    };
    let harness = start_harness(cfg).await.expect("startup should succeed");
    assert_eq!(harness.addr.ip(), requested.ip());
    assert_ne!(harness.addr.port(), 0);
    harness.shutdown().await.expect("shutdown should succeed");
}

#[rstest]
#[tokio::test]
async fn start_harness_with_supported_replay_cassette_succeeds() -> HarnessResult<()> {
    let cassette_name = unique_cassette_name("replay-supported");
    seed_replay_cassette(&cassette_name)?;

    let harness = start_harness(replay_config(cassette_name.clone()))
        .await
        .expect("supported replay cassette should start");

    assert_eq!(
        harness.cassette_path,
        Utf8PathBuf::from("target/test-harness").join(cassette_name),
    );
    harness.shutdown().await.expect("shutdown should succeed");
    Ok(())
}

#[rstest]
#[tokio::test]
async fn start_harness_replay_missing_cassette_fails() {
    let cassette_name = unique_cassette_name("replay-missing");

    let error = start_harness(replay_config(cassette_name.clone()))
        .await
        .expect_err("missing replay cassette should fail");

    assert!(matches!(
        error,
        HarnessError::CassetteNotFound { cassette_name: found }
            if found == cassette_name
    ));
}

#[rstest]
#[tokio::test]
async fn start_harness_replay_unsupported_cassette_version_fails() -> HarnessResult<()> {
    let supported = CassetteFormatVersion::SUPPORTED.as_u32();
    let cassette_name = unique_cassette_name("replay-unsupported");
    let cassette_path = seed_replay_cassette(&cassette_name)?;
    write_cassette_bytes(&cassette_path, br#"{"format_version":9,"interactions":[]}"#)?;

    let error = start_harness(replay_config(cassette_name))
        .await
        .expect_err("unsupported replay cassette should fail");

    assert!(matches!(
        error,
        HarnessError::UnsupportedCassetteFormatVersion {
            found: 9,
            supported: found_supported,
        }
        if found_supported == supported
    ));
    Ok(())
}

#[rstest]
#[tokio::test]
async fn start_harness_verify_mode_returns_not_yet_implemented() -> HarnessResult<()> {
    let cassette_name = unique_cassette_name("verify-nyi");
    seed_replay_cassette(&cassette_name)?;
    let cfg = HarnessConfig {
        mode: config::Mode::Verify,
        ..replay_config(cassette_name)
    };

    let result = start_harness(cfg).await;

    assert!(
        matches!(result, Err(HarnessError::ModeNotYetImplemented { .. })),
        "expected ModeNotYetImplemented, got {result:?}"
    );
    Ok(())
}

fn base_config(cassette_name: String) -> HarnessConfig {
    HarnessConfig {
        listen: SocketAddr::from(([127, 0, 0, 1], 0)).into(),
        cassette_dir: Utf8PathBuf::from("target/test-harness"),
        cassette_name,
        ..HarnessConfig::default()
    }
}

fn record_config(cassette_name: String) -> HarnessConfig {
    HarnessConfig {
        mode: config::Mode::Record,
        upstream: Some(config::UpstreamConfig::default()),
        ..base_config(cassette_name)
    }
}

fn replay_config(cassette_name: String) -> HarnessConfig {
    HarnessConfig {
        mode: config::Mode::Replay,
        ..base_config(cassette_name)
    }
}

fn unique_cassette_name(prefix: &str) -> String {
    let index = NEXT_TEST_CASSETTE.fetch_add(1, Ordering::Relaxed);
    let uuid = Uuid::new_v4();
    format!("{prefix}-{index}-{uuid}")
}

fn seed_replay_cassette(cassette_name: &str) -> HarnessResult<Utf8PathBuf> {
    let cassette_path = Utf8PathBuf::from("target/test-harness").join(cassette_name);
    write_cassette_bytes(&cassette_path, br#"{"format_version":1,"interactions":[]}"#)?;
    Ok(cassette_path)
}

fn write_cassette_bytes(cassette_path: &Utf8PathBuf, body: &[u8]) -> HarnessResult<()> {
    let root = Dir::open_ambient_dir(".", ambient_authority())?;
    let parent = cassette_path
        .parent()
        .ok_or_else(|| HarnessError::InvalidConfig {
            message: format!("cassette path needs parent directory: {cassette_path}"),
        })?;
    root.create_dir_all(parent)?;
    let parent_dir = root.open_dir(parent)?;
    let mut file = parent_dir.create(cassette_file_name(cassette_path)?)?;
    std::io::Write::write_all(&mut file, body)?;
    file.sync_all()?;
    Ok(())
}

fn cassette_file_name(cassette_path: &Utf8PathBuf) -> HarnessResult<&str> {
    cassette_path
        .file_name()
        .ok_or_else(|| HarnessError::InvalidConfig {
            message: "cassette path should include a file name".to_owned(),
        })
}
