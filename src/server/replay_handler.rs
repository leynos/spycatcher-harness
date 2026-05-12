//! Axum route handler and response builder for replay-mode chat completions.
//!
//! The handler translates inbound HTTP requests into adapter-neutral exchange
//! data and renders replay outcomes without exposing Axum types to cassette
//! matching or replay policy.

use axum::body::Bytes;
use axum::extract::{OriginalUri, State};
use axum::http::{
    HeaderMap, HeaderName, HeaderValue, Method, Response, StatusCode, Uri, header::CONTENT_TYPE,
};
use log::warn;
use serde_json::json;
use tracing::{debug, info};

use crate::cassette::{InteractionPosition, MismatchDiagnostic};
use crate::http_exchange::{ObservedRequest, parse_json_bytes, selected_request_headers};
use crate::protocol::CHAT_COMPLETIONS_PATH;
use crate::replay::{ReplayError, ReplayResponse};

use super::replay::ReplayAppState;

/// Axum route handler for replay-mode chat completions playback.
pub(crate) async fn replay_chat_completions_handler(
    State(state): State<ReplayAppState>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    body: Bytes,
) -> Response<axum::body::Body> {
    log_replay_request(&axum::http::Method::POST, &uri);
    let body_bytes = body.to_vec();
    let request = ObservedRequest {
        method: "POST".to_owned(),
        path: CHAT_COMPLETIONS_PATH.to_owned(),
        query: uri.query().unwrap_or_default().to_owned(),
        headers: selected_request_headers(&headers),
        forward_headers: Vec::new(),
        parsed_json: parse_json_bytes(&body_bytes),
        body: body_bytes,
    };
    match state.service.handle_chat_completions(request) {
        Ok(replayed) => build_replay_response(replayed),
        Err(error) => build_replay_error_response(&error),
    }
}

fn log_replay_request(method: &Method, uri: &Uri) {
    debug!(
        target: "spycatcher.harness.replay",
        method = %method,
        path = uri.path(),
        "replay-mode request received",
    );
    info!(
        method = %method,
        path = uri.path(),
        "chat completions replay request received method={method} path={path}",
        path = uri.path(),
    );
}

fn build_replay_response(response: ReplayResponse) -> Response<axum::body::Body> {
    let mut built = Response::new(axum::body::Body::from(response.body));
    *built.status_mut() = replay_status_code(response.status);
    for (name, value) in response.headers {
        match (
            HeaderName::try_from(name.as_str()),
            HeaderValue::from_bytes(value.as_bytes()),
        ) {
            (Ok(header_name), Ok(header_value)) => {
                built.headers_mut().append(header_name, header_value);
            }
            _ => {
                warn!(
                    target: "spycatcher.harness.replay",
                    "dropping unparseable replay response header name={name:?} value_len={}",
                    value.len()
                );
            }
        }
    }
    built
}

fn replay_status_code(status: u16) -> StatusCode {
    if (100..=599).contains(&status) {
        StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_GATEWAY)
    } else {
        StatusCode::BAD_GATEWAY
    }
}

fn build_replay_error_response(error: &ReplayError) -> Response<axum::body::Body> {
    let (status, body) = match error {
        ReplayError::Mismatch(diagnostic) => (
            StatusCode::CONFLICT,
            mismatch_error_body(diagnostic).into_bytes(),
        ),
        ReplayError::UnsupportedStream => (
            StatusCode::NOT_IMPLEMENTED,
            json_error_body(
                "unsupported_stream",
                "streaming chat completions replay is not implemented yet",
            )
            .into_bytes(),
        ),
        ReplayError::MalformedJson => (
            StatusCode::BAD_REQUEST,
            json_error_body(
                "malformed_json",
                "chat completions replay requires a valid JSON request body",
            )
            .into_bytes(),
        ),
        ReplayError::Internal => (
            StatusCode::BAD_GATEWAY,
            json_error_body("internal", "replay request failed").into_bytes(),
        ),
    };
    let mut response = Response::new(axum::body::Body::from(body));
    *response.status_mut() = status;
    response
        .headers_mut()
        .insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    response
}

fn json_error_body(kind: &str, message: &str) -> String {
    json!({
        "error": {
            "kind": kind,
            "message": message,
        }
    })
    .to_string()
}

fn mismatch_error_body(diagnostic: &MismatchDiagnostic) -> String {
    json!({
        "error": {
            "kind": "request_mismatch",
            "message": "replay request did not match the cassette",
            "position": interaction_position_json(diagnostic.position),
            "expected_hash": diagnostic.expected_hash,
            "observed_hash": diagnostic.observed_hash,
            "diff_summary": diagnostic.diff_summary,
        }
    })
    .to_string()
}

fn interaction_position_json(position: InteractionPosition) -> serde_json::Value {
    match position {
        InteractionPosition::Expected(index) => json!({
            "kind": "expected",
            "index": index,
        }),
        InteractionPosition::Exhausted(count) => json!({
            "kind": "exhausted",
            "count": count,
        }),
        InteractionPosition::KeyedMiss(count) => json!({
            "kind": "keyed_miss",
            "count": count,
        }),
    }
}

#[cfg(test)]
mod tests {
    //! Unit tests for replay-mode HTTP response construction.

    use super::*;

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
            body: b"recorded bytes".to_vec(),
        };

        let built = build_replay_response(response);

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
            body: Vec::new(),
        };

        let built = build_replay_response(response);

        assert!(built.headers().get("bad header").is_none());
    }

    #[rstest::rstest]
    fn build_replay_response_falls_back_to_502_for_invalid_status() {
        let response = ReplayResponse {
            status: 999,
            headers: vec![],
            body: Vec::new(),
        };

        let built = build_replay_response(response);

        assert_eq!(built.status(), StatusCode::BAD_GATEWAY);
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
        assert_eq!(
            error
                .get("expected_hash")
                .and_then(serde_json::Value::as_str),
            Some("expected"),
        );
        assert_eq!(
            error
                .get("observed_hash")
                .and_then(serde_json::Value::as_str),
            Some("observed"),
        );
        assert_eq!(
            error
                .get("diff_summary")
                .and_then(serde_json::Value::as_str),
            Some("changed model"),
        );
    }

    #[tokio::test]
    async fn mismatch_error_response_matches_snapshot() -> Result<(), Box<dyn std::error::Error>> {
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
    async fn unsupported_stream_error_response_matches_snapshot()
    -> Result<(), Box<dyn std::error::Error>> {
        assert_error_response_snapshot(
            "unsupported_stream",
            build_replay_error_response(&ReplayError::UnsupportedStream),
            StatusCode::NOT_IMPLEMENTED,
        )
        .await
    }

    #[tokio::test]
    async fn malformed_json_error_response_matches_snapshot()
    -> Result<(), Box<dyn std::error::Error>> {
        assert_error_response_snapshot(
            "malformed_json",
            build_replay_error_response(&ReplayError::MalformedJson),
            StatusCode::BAD_REQUEST,
        )
        .await
    }

    #[tokio::test]
    async fn internal_error_response_matches_snapshot() -> Result<(), Box<dyn std::error::Error>> {
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
    ) -> Result<(), Box<dyn std::error::Error>> {
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
}
