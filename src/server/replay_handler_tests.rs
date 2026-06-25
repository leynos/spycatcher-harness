//! Unit tests for replay-mode HTTP response construction.

use axum::http::{HeaderValue, Response, StatusCode, header::CONTENT_TYPE};

use crate::cassette::{InteractionPosition, MismatchDiagnostic};
use crate::protocol::{CHAT_COMPLETIONS_PATH, CHAT_COMPLETIONS_PROTOCOL_ID};
use crate::replay::{ReplayBody, ReplayError, ReplayResponse};
use crate::replay_observability::ReplayMetricLabels;

use super::{build_replay_error_response, build_replay_response};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[rstest::rstest]
#[tokio::test]
async fn build_replay_response_propagates_status_headers_and_body() {
    let response = ReplayResponse {
        status: 202,
        headers: vec![
            ("content-type".to_owned(), "application/json".to_owned()),
            ("x-repeat".to_owned(), "one".to_owned()),
            ("x-repeat".to_owned(), "two".to_owned()),
        ],
        body: ReplayBody::OneShot(b"recorded bytes".to_vec()),
    };

    let built = build_replay_response(response, &labels());

    assert_eq!(built.status(), StatusCode::ACCEPTED);
    assert_eq!(built.headers().get_all("x-repeat").iter().count(), 2);
    let body = axum::body::to_bytes(built.into_body(), 1024)
        .await
        .expect("body readable");
    assert_eq!(body.as_ref(), b"recorded bytes");
}

#[rstest::rstest]
fn build_replay_response_drops_invalid_headers() {
    let response = ReplayResponse {
        status: 200,
        headers: vec![("bad header".to_owned(), "value".to_owned())],
        body: ReplayBody::OneShot(Vec::new()),
    };

    let built = build_replay_response(response, &labels());

    assert!(built.headers().get("bad header").is_none());
}

#[rstest::rstest]
fn build_replay_response_falls_back_to_502_for_invalid_status() {
    let response = ReplayResponse {
        status: 999,
        headers: vec![],
        body: ReplayBody::OneShot(Vec::new()),
    };

    let built = build_replay_response(response, &labels());

    assert_eq!(built.status(), StatusCode::BAD_GATEWAY);
}

#[rstest::rstest]
#[tokio::test]
async fn build_replay_response_renders_stream_events_and_default_content_type() {
    let response = ReplayResponse {
        status: 201,
        headers: vec![("x-stream".to_owned(), "yes".to_owned())],
        body: ReplayBody::Events(vec![
            crate::cassette::StreamEvent::Comment {
                text: "OPENROUTER PROCESSING".to_owned(),
            },
            crate::cassette::StreamEvent::Data {
                raw: "[DONE]".to_owned(),
                parsed_json: None,
            },
        ]),
    };

    let built = build_replay_response(response, &labels());

    assert_eq!(built.status(), StatusCode::CREATED);
    assert_eq!(
        built.headers().get(CONTENT_TYPE),
        Some(&HeaderValue::from_static("text/event-stream")),
    );
    assert_eq!(
        built.headers().get("x-stream"),
        Some(&HeaderValue::from_static("yes")),
    );
    let body = axum::body::to_bytes(built.into_body(), 1024)
        .await
        .expect("body readable");
    assert_eq!(
        body.as_ref(),
        b": OPENROUTER PROCESSING\n\ndata: [DONE]\n\n"
    );
}

#[rstest::rstest]
#[tokio::test]
async fn mismatch_error_response_contains_structured_diagnostic() {
    let diagnostic = MismatchDiagnostic {
        position: InteractionPosition::Expected(3),
        expected_hash: "expected".to_owned(),
        observed_hash: "observed".to_owned(),
        diff_summary: "changed model".to_owned(),
    };

    let response = build_replay_error_response(&ReplayError::Mismatch(diagnostic));

    assert_eq!(response.status(), StatusCode::CONFLICT);
    let body = axum::body::to_bytes(response.into_body(), 2048)
        .await
        .expect("body readable");
    let value: serde_json::Value = serde_json::from_slice(&body).expect("body should be JSON");
    let error = value.get("error").expect("error field should be present");
    let position = error
        .get("position")
        .expect("position field should be present");
    assert_eq!(
        error.get("kind").and_then(serde_json::Value::as_str),
        Some("request_mismatch")
    );
    assert_eq!(
        position.get("kind").and_then(serde_json::Value::as_str),
        Some("expected"),
    );
    assert_eq!(
        position.get("index").and_then(serde_json::Value::as_u64),
        Some(3)
    );
    assert_eq!(json_str(error, "expected_hash"), Some("expected"));
    assert_eq!(json_str(error, "observed_hash"), Some("observed"));
    assert_eq!(json_str(error, "diff_summary"), Some("changed model"));
}

#[tokio::test]
async fn mismatch_error_response_matches_snapshot() -> TestResult {
    let diagnostic = MismatchDiagnostic {
        position: InteractionPosition::Expected(3),
        expected_hash: "expected".to_owned(),
        observed_hash: "observed".to_owned(),
        diff_summary: "changed model".to_owned(),
    };
    assert_error_response_snapshot(
        "mismatch",
        build_replay_error_response(&ReplayError::Mismatch(diagnostic)),
        StatusCode::CONFLICT,
    )
    .await
}

#[tokio::test]
async fn malformed_json_error_response_matches_snapshot() -> TestResult {
    assert_error_response_snapshot(
        "malformed_json",
        build_replay_error_response(&ReplayError::MalformedJson),
        StatusCode::BAD_REQUEST,
    )
    .await
}

#[tokio::test]
async fn stream_cassette_required_error_response_matches_snapshot() -> TestResult {
    assert_error_response_snapshot(
        "stream_cassette_required",
        build_replay_error_response(&ReplayError::StreamCassetteRequiredForStreamRequest),
        StatusCode::NOT_IMPLEMENTED,
    )
    .await
}

#[tokio::test]
async fn internal_error_response_matches_snapshot() -> TestResult {
    assert_error_response_snapshot(
        "internal",
        build_replay_error_response(&ReplayError::Internal),
        StatusCode::BAD_GATEWAY,
    )
    .await
}

async fn assert_error_response_snapshot(
    name: &str,
    response: Response<axum::body::Body>,
    expected_status: StatusCode,
) -> TestResult {
    assert_eq!(response.status(), expected_status);
    let body = axum::body::to_bytes(response.into_body(), 2048).await?;
    let value = serde_json::json!({
        "case": name,
        "status": expected_status.as_u16(),
        "body": serde_json::from_slice::<serde_json::Value>(&body)?,
    });
    insta::assert_json_snapshot!(name, value);
    Ok(())
}

fn labels() -> ReplayMetricLabels {
    ReplayMetricLabels::new(
        "test-cassette".to_owned(),
        CHAT_COMPLETIONS_PROTOCOL_ID,
        CHAT_COMPLETIONS_PATH,
    )
}

fn json_str<'a>(value: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(serde_json::Value::as_str)
}
