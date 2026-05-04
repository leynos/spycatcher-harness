//! Adapter-neutral HTTP exchange types and header-handling helpers.
//!
//! This module keeps request and response capture policy in one place so the
//! server and upstream adapters agree on what gets forwarded, returned, and
//! persisted.

use axum::http::{HeaderMap, HeaderName};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde_json::Value;

use crate::config::RedactionConfig;

const HOP_BY_HOP_HEADERS: &[&str] = &[
    "connection",
    "keep-alive",
    "proxy-authenticate",
    "proxy-authorization",
    "te",
    "trailer",
    "transfer-encoding",
    "upgrade",
];

const REQUEST_ONLY_EXCLUDED_HEADERS: &[&str] = &["host", "content-length", "accept-encoding"];
const RESPONSE_ONLY_EXCLUDED_HEADERS: &[&str] = &["content-length"];

/// Captured inbound request data independent of the HTTP framework.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ObservedRequest {
    /// Uppercase HTTP method name.
    pub method: String,
    /// Request path without query parameters.
    pub path: String,
    /// Raw query string in observed order.
    pub query: String,
    /// Selected headers in observed order.
    pub headers: Vec<(String, String)>,
    /// Raw request body bytes.
    pub body: Vec<u8>,
    /// Parsed JSON body when the bytes form valid JSON.
    pub parsed_json: Option<Value>,
}

/// Captured upstream response data independent of the HTTP framework.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ObservedResponse {
    /// HTTP status code.
    pub status: u16,
    /// Selected headers in observed order.
    pub headers: Vec<(String, String)>,
    /// Raw body bytes.
    pub body: Vec<u8>,
    /// Parsed JSON body when the bytes form valid JSON.
    pub parsed_json: Option<Value>,
}

/// Response data returned from the service to the inbound adapter.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ProxyResponse {
    /// HTTP status code.
    pub status: u16,
    /// Selected headers in observed order.
    pub headers: Vec<(String, String)>,
    /// Raw body bytes.
    pub body: Vec<u8>,
}

/// Parses bytes as JSON, returning `None` when parsing fails.
#[must_use]
pub(crate) fn parse_json_bytes(bytes: &[u8]) -> Option<Value> {
    serde_json::from_slice(bytes).ok()
}

/// Selects request headers that are meaningful for proxying and persistence.
#[must_use]
pub(crate) fn selected_request_headers(headers: &HeaderMap) -> Vec<(String, String)> {
    selected_headers(headers, REQUEST_ONLY_EXCLUDED_HEADERS)
}

/// Selects response headers that are meaningful for proxying and persistence.
#[must_use]
pub(crate) fn selected_response_headers(headers: &HeaderMap) -> Vec<(String, String)> {
    selected_headers(headers, RESPONSE_ONLY_EXCLUDED_HEADERS)
}

fn selected_headers(headers: &HeaderMap, excluded: &[&str]) -> Vec<(String, String)> {
    headers
        .iter()
        .filter(|(name, _)| should_keep_header(name, excluded))
        .map(|(name, value)| (name.as_str().to_owned(), header_value_string(value)))
        .collect()
}

fn header_value_string(value: &axum::http::HeaderValue) -> String {
    value.to_str().map_or_else(
        |_| format!("B64:{}", BASE64.encode(value.as_bytes())),
        ToOwned::to_owned,
    )
}

fn should_keep_header(name: &HeaderName, excluded: &[&str]) -> bool {
    !HOP_BY_HOP_HEADERS
        .iter()
        .chain(excluded.iter())
        .any(|candidate| name.as_str().eq_ignore_ascii_case(candidate))
}

/// Applies case-insensitive configured header redaction before persistence.
#[must_use]
pub(crate) fn redact_headers(
    headers: &[(String, String)],
    redaction: &RedactionConfig,
) -> Vec<(String, String)> {
    headers
        .iter()
        .filter(|(name, _)| should_persist_header(name, redaction))
        .cloned()
        .collect()
}

fn should_persist_header(name: &str, redaction: &RedactionConfig) -> bool {
    !redaction
        .drop_headers
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(name))
}

#[cfg(test)]
mod tests {
    //! Unit tests for header filtering and JSON parsing helpers.

    use super::*;
    use rstest::rstest;

    use crate::config::RedactionConfig;

    #[rstest]
    fn selected_request_headers_drop_hop_by_hop_and_framing_headers() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "content-type",
            "application/json".parse().expect("valid header"),
        );
        headers.insert("host", "example.invalid".parse().expect("valid header"));
        headers.insert("connection", "keep-alive".parse().expect("valid header"));
        headers.insert(
            "authorization",
            "Bearer keep-me".parse().expect("valid header"),
        );

        assert_eq!(
            selected_request_headers(&headers),
            vec![
                ("content-type".to_owned(), "application/json".to_owned()),
                ("authorization".to_owned(), "Bearer keep-me".to_owned()),
            ],
        );
    }

    #[rstest]
    fn redact_headers_matches_names_case_insensitively() {
        let headers = vec![
            ("Authorization".to_owned(), "Bearer secret".to_owned()),
            ("x-trace-id".to_owned(), "trace-123".to_owned()),
            ("X-API-Key".to_owned(), "secret-key".to_owned()),
        ];
        let redaction = RedactionConfig {
            drop_headers: vec!["authorization".to_owned(), "x-api-key".to_owned()],
        };

        assert_eq!(
            redact_headers(&headers, &redaction),
            vec![("x-trace-id".to_owned(), "trace-123".to_owned())],
        );
    }

    #[rstest]
    fn parse_json_bytes_returns_none_for_invalid_json() {
        assert_eq!(parse_json_bytes(br#"{"unterminated": true"#), None);
    }
}
