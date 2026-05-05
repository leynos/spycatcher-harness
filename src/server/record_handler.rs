//! Axum route handler and response builder for record-mode chat completions.
//!
//! The handler translates `axum` request data into adapter-neutral types and
//! delegates to the record service, while `build_proxy_response` assembles
//! the downstream HTTP response from a [`ProxyResponse`].

use axum::body::Bytes;
use axum::extract::{OriginalUri, State};
use axum::http::{HeaderMap, HeaderName, HeaderValue, Response, StatusCode};
use log::warn;

use crate::http_exchange::{
    ObservedRequest, ProxyResponse, parse_json_bytes, selected_request_headers,
};
use crate::protocol::CHAT_COMPLETIONS_PATH;

use super::record::{RecordAppState, RecordError};

/// Axum route handler for record-mode chat completions proxying.
pub(crate) async fn record_chat_completions_handler(
    State(state): State<RecordAppState>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response<axum::body::Body>, RecordError> {
    let body_bytes = body.to_vec();
    let request = ObservedRequest {
        method: "POST".to_owned(),
        path: CHAT_COMPLETIONS_PATH.to_owned(),
        query: uri.query().unwrap_or_default().to_owned(),
        headers: selected_request_headers(&headers),
        parsed_json: parse_json_bytes(&body_bytes),
        body: body_bytes,
    };
    let proxied = state.service.handle_chat_completions(request).await?;
    Ok(build_proxy_response(proxied))
}

fn build_proxy_response(response: ProxyResponse) -> Response<axum::body::Body> {
    let mut built = Response::new(axum::body::Body::from(response.body));
    *built.status_mut() = StatusCode::from_u16(response.status).unwrap_or(StatusCode::BAD_GATEWAY);
    for (name, value) in response.headers {
        match (
            HeaderName::try_from(name.as_str()),
            HeaderValue::from_str(&value),
        ) {
            (Ok(header_name), Ok(header_value)) => {
                built.headers_mut().append(header_name, header_value);
            }
            _ => {
                warn!(
                    target: "spycatcher.harness.record",
                    "dropping unparseable proxy response header name={name:?} value={value:?}"
                );
            }
        }
    }
    built
}
