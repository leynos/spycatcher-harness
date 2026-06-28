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
use serde_json::json;
use tracing::{debug, info, info_span, warn};

use crate::cassette::{InteractionPosition, MismatchDiagnostic};
use crate::http_exchange::{ObservedRequest, parse_json_bytes, selected_request_headers};
use crate::protocol::{
    CHAT_COMPLETIONS_PATH, CHAT_COMPLETIONS_PROTOCOL_ID, is_streaming_chat_completions_request,
};
use crate::replay::{ReplayBody, ReplayError, ReplayResponse};
use crate::replay_observability::MODE_REPLAY;

use super::replay::ReplayAppState;
use super::replay_stream::build_stream_body;

/// Axum route handler for replay-mode chat completions playback.
pub(crate) async fn replay_chat_completions_handler(
    State(state): State<ReplayAppState>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    body: Bytes,
) -> Response<axum::body::Body> {
    let body_bytes = body.to_vec();
    let parsed_json = parse_json_bytes(&body_bytes);
    let is_stream_request = is_streaming_chat_completions_request(parsed_json.as_ref());
    let span = info_span!(
        "chat_completions_replay",
        mode = MODE_REPLAY,
        protocol = CHAT_COMPLETIONS_PROTOCOL_ID,
        route = CHAT_COMPLETIONS_PATH,
        is_stream_request,
    );
    let _span_guard = span.enter();
    log_replay_request(&axum::http::Method::POST, &uri);
    let request = ObservedRequest {
        method: "POST".to_owned(),
        path: CHAT_COMPLETIONS_PATH.to_owned(),
        query: uri.query().unwrap_or_default().to_owned(),
        headers: selected_request_headers(&headers),
        forward_headers: Vec::new(),
        parsed_json,
        body: body_bytes,
    };
    let labels = state.service.metric_labels();
    match state.service.handle_chat_completions(request) {
        Ok(replayed) => build_replay_response(replayed, &labels),
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

fn build_replay_response(
    response: ReplayResponse,
    labels: &crate::replay_observability::ReplayMetricLabels,
) -> Response<axum::body::Body> {
    let ReplayResponse {
        status,
        headers,
        body: replay_body,
    } = response;
    let (is_stream, axum_body) = match replay_body {
        ReplayBody::OneShot(bytes) => (false, axum::body::Body::from(bytes)),
        ReplayBody::Events(events) => (true, build_stream_body(events, labels)),
    };
    let mut built = Response::new(axum_body);
    *built.status_mut() = replay_status_code(status);
    let mut has_content_type = false;
    for (name, value) in headers {
        add_replay_header(&mut built, &mut has_content_type, &name, &value);
    }
    if is_stream && !has_content_type {
        built
            .headers_mut()
            .insert(CONTENT_TYPE, HeaderValue::from_static("text/event-stream"));
    }
    built
}

fn add_replay_header(
    built: &mut Response<axum::body::Body>,
    has_content_type: &mut bool,
    name: &str,
    value: &str,
) {
    match (
        HeaderName::try_from(name),
        HeaderValue::from_bytes(value.as_bytes()),
    ) {
        (Ok(header_name), Ok(header_value)) => {
            *has_content_type |= header_name == CONTENT_TYPE;
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
        ReplayError::StreamCassetteRequiredForStreamRequest => (
            StatusCode::NOT_IMPLEMENTED,
            json_error_body(
                "stream_cassette_required",
                "streaming chat completions replay requires a recorded stream response",
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
#[path = "replay_handler_tests.rs"]
mod tests;
