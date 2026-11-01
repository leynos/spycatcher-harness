//! Unit tests for record-mode orchestration.
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use rstest::rstest;

use super::{ProxyBody, RecordError, RecordedResponse, record_tests_helpers::*};
use crate::{
    config::UpstreamKind,
    server::record_metadata::{Clock, MetadataFactory, SessionMetadata, SystemClock},
};

#[rstest]
#[tokio::test]
async fn missing_api_key_does_not_append() -> eyre::Result<()> {
    assert_error_does_not_append(
        "missing-key",
        FakeEnvProvider(None),
        sample_request(None),
        |error| assert_eq!(error, RecordError::MissingApiKeyNotConfigured),
    )
    .await?;
    Ok(())
}
#[rstest]
fn resolve_api_key_returns_key_when_present() -> eyre::Result<()> {
    let fixture = service_fixture_ephemeral(FakeEnvProvider(Some("my-secret".to_owned())))?;
    let api_key = fixture.service.resolve_api_key().map_err(|error| {
        eyre::eyre!("API key should resolve when env provider has a value: {error:?}")
    })?;

    eyre::ensure!(
        api_key == "my-secret",
        "API key should match the configured environment value; got {:?}",
        api_key
    );
    Ok(())
}
#[rstest]
fn resolve_api_key_errors_when_absent() -> eyre::Result<()> {
    let fixture = service_fixture_ephemeral(FakeEnvProvider(None))?;

    eyre::ensure!(
        matches!(
            fixture.service.resolve_api_key(),
            Err(RecordError::MissingApiKeyNotConfigured)
        ),
        "missing API key should return the configuration error"
    );
    Ok(())
}
#[rstest]
#[tokio::test]
async fn upstream_transport_failure_does_not_append() -> eyre::Result<()> {
    let cassette = cassette_fixture("upstream-fail")?;
    let service = service_fixture(
        &cassette.path,
        FakeUpstream { response: Err(()) },
        FakeEnvProvider(Some("token".to_owned())),
    )?;

    let result = service.handle_chat_completions(sample_request(None)).await;

    eyre::ensure!(
        matches!(result, Err(RecordError::Internal)),
        "upstream transport failure should return RecordError::Internal"
    );
    let persisted = load_cassette(&cassette.path)?;
    eyre::ensure!(
        persisted.interactions.is_empty(),
        "no interaction should be recorded on upstream failure"
    );
    Ok(())
}
#[rstest]
fn system_clock_produces_rfc3339_string() {
    let ts = SystemClock
        .now_rfc3339()
        .expect("system clock should produce an RFC 3339 timestamp");

    assert_ne!(ts, "");
    assert!(ts.contains('T'));
}
#[rstest]
fn session_metadata_uses_injected_clock() {
    let metadata = SessionMetadata::with_clock(UpstreamKind::OpenRouter, Arc::new(FixedClock))
        .create()
        .expect("session metadata should be created with fixed clock");

    assert_eq!(metadata.recorded_at, "2024-01-01T00:00:00Z");
}
#[rstest]
fn session_metadata_uses_injected_session_start() {
    let injected_offset = Duration::from_millis(100);
    let session_start = Instant::now()
        .checked_sub(injected_offset)
        .expect("fixed offset should be within Instant range");
    let metadata = SessionMetadata::with_clock_and_start(
        UpstreamKind::OpenRouter,
        Arc::new(FixedClock),
        session_start,
    )
    .create()
    .expect("session metadata should be created with fixed start");

    assert!(
        u128::from(metadata.relative_offset_ms) >= injected_offset.as_millis(),
        "expected relative offset to be at least {} ms, got {} ms",
        injected_offset.as_millis(),
        u128::from(metadata.relative_offset_ms)
    );
}
#[rstest]
#[tokio::test]
async fn invalid_json_response_keeps_exact_bytes() -> eyre::Result<()> {
    let cassette = cassette_fixture("invalid-json")?;
    let service = service_fixture(
        &cassette.path,
        FakeUpstream {
            response: Ok(sample_response(br#"{"broken": true"#)),
        },
        FakeEnvProvider(Some("token".to_owned())),
    )?;

    let proxied = service
        .handle_chat_completions(sample_request(None))
        .await
        .map_err(|error| eyre::eyre!("request should succeed: {error:?}"))?;

    let ProxyBody::Buffered(proxied_body) = proxied.body else {
        eyre::bail!("expected buffered response");
    };
    let expected_body = br#"{"broken": true"#.to_vec();
    eyre::ensure!(
        proxied_body == expected_body,
        "buffered response should preserve the exact invalid JSON bytes; got {:?}, expected {:?}",
        proxied_body,
        expected_body
    );
    let persisted = load_cassette(&cassette.path)?;
    let interaction = persisted
        .interactions
        .first()
        .ok_or_else(|| eyre::eyre!("expected one recorded interaction"))?;
    let RecordedResponse::NonStream {
        body, parsed_json, ..
    } = &interaction.response
    else {
        eyre::bail!("expected non-stream response");
    };
    eyre::ensure!(
        body == &expected_body,
        "persisted response should preserve the exact invalid JSON bytes; got {:?}, expected {:?}",
        body,
        expected_body
    );
    eyre::ensure!(
        parsed_json.is_none(),
        "invalid JSON response should not have a parsed value"
    );
    Ok(())
}
