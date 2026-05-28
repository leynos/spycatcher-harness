//! Unit tests for record-mode orchestration.
use super::record_tests_helpers::*;
use super::{ProxyBody, RecordError, RecordedResponse};
use rstest::rstest;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::config::UpstreamKind;
use crate::server::record_metadata::{Clock, MetadataFactory, SessionMetadata, SystemClock};

#[rstest]
#[tokio::test]
async fn missing_api_key_does_not_append() {
    assert_error_does_not_append(
        "missing-key",
        FakeEnvProvider(None),
        sample_request(None),
        |error| assert_eq!(error, RecordError::MissingApiKeyNotConfigured),
    )
    .await;
}
#[rstest]
fn resolve_api_key_returns_key_when_present() {
    let fixture = service_fixture_ephemeral(FakeEnvProvider(Some("my-secret".to_owned())));

    assert_eq!(
        fixture
            .service
            .resolve_api_key()
            .expect("API key should resolve when env provider has a value"),
        "my-secret",
    );
}
#[rstest]
fn resolve_api_key_errors_when_absent() {
    let fixture = service_fixture_ephemeral(FakeEnvProvider(None));

    assert!(matches!(
        fixture.service.resolve_api_key(),
        Err(RecordError::MissingApiKeyNotConfigured)
    ));
}
#[rstest]
#[tokio::test]
async fn upstream_transport_failure_does_not_append() {
    let cassette = cassette_fixture("upstream-fail");
    let service = service_fixture(
        &cassette.path,
        FakeUpstream { response: Err(()) },
        FakeEnvProvider(Some("token".to_owned())),
    );

    let result = service.handle_chat_completions(sample_request(None)).await;

    assert!(
        matches!(result, Err(RecordError::Internal)),
        "upstream transport failure should return RecordError::Internal"
    );
    assert!(
        load_cassette(&cassette.path).interactions.is_empty(),
        "no interaction should be recorded on upstream failure"
    );
}
#[rstest]
fn system_clock_produces_rfc3339_string() {
    let ts = SystemClock
        .now_rfc3339()
        .expect("system clock should produce an RFC 3339 timestamp");

    assert!(!ts.is_empty());
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
async fn invalid_json_response_keeps_exact_bytes() {
    let cassette = cassette_fixture("invalid-json");
    let service = service_fixture(
        &cassette.path,
        FakeUpstream {
            response: Ok(sample_response(br#"{"broken": true"#)),
        },
        FakeEnvProvider(Some("token".to_owned())),
    );

    let proxied = service
        .handle_chat_completions(sample_request(None))
        .await
        .expect("request should succeed");

    let ProxyBody::Buffered(proxied_body) = proxied.body else {
        panic!("expected buffered response");
    };
    assert_eq!(proxied_body, br#"{"broken": true"#.to_vec());
    let persisted = load_cassette(&cassette.path);
    let interaction = persisted
        .interactions
        .first()
        .expect("expected one recorded interaction");
    let RecordedResponse::NonStream {
        body, parsed_json, ..
    } = &interaction.response
    else {
        panic!("expected non-stream response");
    };
    assert_eq!(body, &br#"{"broken": true"#.to_vec());
    assert_eq!(parsed_json, &None);
}
