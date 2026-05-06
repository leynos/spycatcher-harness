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
    ObservedRequest, ProxyResponse, parse_json_bytes, selected_forward_headers,
    selected_request_headers,
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
        forward_headers: selected_forward_headers(&headers),
        parsed_json: parse_json_bytes(&body_bytes),
        body: body_bytes,
    };
    let proxied = state.service.handle_chat_completions(request).await?;
    Ok(build_proxy_response(proxied))
}

fn build_proxy_response(response: ProxyResponse) -> Response<axum::body::Body> {
    let mut built = Response::new(axum::body::Body::from(response.body));
    *built.status_mut() = proxy_status_code(response.status);
    for (name, value) in response.headers {
        match (
            HeaderName::try_from(name.as_str()),
            HeaderValue::from_bytes(&value),
        ) {
            (Ok(header_name), Ok(header_value)) => {
                built.headers_mut().append(header_name, header_value);
            }
            _ => {
                warn!(
                    target: "spycatcher.harness.record",
                    "dropping unparseable proxy response header name={name:?} value_len={}",
                    value.len()
                );
            }
        }
    }
    built
}

fn proxy_status_code(status: u16) -> StatusCode {
    if (100..=599).contains(&status) {
        StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_GATEWAY)
    } else {
        StatusCode::BAD_GATEWAY
    }
}

#[cfg(test)]
mod tests {
    //! Unit tests for record-mode proxy response construction.

    use axum::http::StatusCode;

    use super::*;
    use crate::http_exchange::ProxyResponse;

    #[rstest::rstest]
    #[tokio::test]
    async fn build_proxy_response_propagates_status_and_body() {
        let proxy = ProxyResponse {
            status: 201,
            headers: vec![],
            body: b"hello".to_vec(),
        };

        let response = build_proxy_response(proxy);

        assert_eq!(response.status(), StatusCode::CREATED);
        let body = axum::body::to_bytes(response.into_body(), 1024)
            .await
            .expect("body readable");
        assert_eq!(body.as_ref(), b"hello");
    }

    #[rstest::rstest]
    #[tokio::test]
    async fn build_proxy_response_sets_headers() {
        let proxy = ProxyResponse {
            status: 200,
            headers: vec![("content-type".to_owned(), b"application/json".to_vec())],
            body: b"{}".to_vec(),
        };

        let response = build_proxy_response(proxy);

        assert_eq!(
            response
                .headers()
                .get("content-type")
                .and_then(|value| value.to_str().ok()),
            Some("application/json"),
        );
    }

    #[rstest::rstest]
    #[tokio::test]
    async fn build_proxy_response_falls_back_to_502_for_invalid_status() {
        let proxy = ProxyResponse {
            status: 999,
            headers: vec![],
            body: vec![],
        };

        let response = build_proxy_response(proxy);

        assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    }
}
