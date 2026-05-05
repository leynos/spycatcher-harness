//! Adapter-neutral HTTP exchange types and header-handling helpers.
//!
//! This module keeps request and response capture policy in one place so the
//! server and upstream adapters agree on what gets forwarded, returned, and
//! persisted.

use std::collections::HashSet;

use axum::http::{HeaderMap, HeaderName};
use percent_encoding::{NON_ALPHANUMERIC, percent_encode};
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
    /// Selected inbound headers as raw bytes for upstream forwarding.
    pub forward_headers: Vec<(String, Vec<u8>)>,
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
    /// Selected headers in observed order for cassette persistence.
    pub headers: Vec<(String, String)>,
    /// Selected headers as raw bytes for downstream proxying.
    pub proxy_headers: Vec<(String, Vec<u8>)>,
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
    /// Selected headers as raw bytes in observed order.
    pub headers: Vec<(String, Vec<u8>)>,
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

/// Selects request headers for upstream forwarding without re-encoding values.
#[must_use]
pub(crate) fn selected_forward_headers(headers: &HeaderMap) -> Vec<(String, Vec<u8>)> {
    selected_header_bytes(headers, REQUEST_ONLY_EXCLUDED_HEADERS)
}

/// Selects response headers that are meaningful for proxying and persistence.
#[must_use]
pub(crate) fn selected_response_headers(headers: &HeaderMap) -> Vec<(String, String)> {
    selected_headers(headers, RESPONSE_ONLY_EXCLUDED_HEADERS)
}

/// Selects response headers for downstream proxying without re-encoding values.
#[must_use]
pub(crate) fn selected_response_proxy_headers(headers: &HeaderMap) -> Vec<(String, Vec<u8>)> {
    selected_header_bytes(headers, RESPONSE_ONLY_EXCLUDED_HEADERS)
}

fn selected_header_bytes(headers: &HeaderMap, excluded: &[&str]) -> Vec<(String, Vec<u8>)> {
    let connection_tokens = parse_connection_tokens(headers);
    headers
        .iter()
        .filter(|(name, _)| should_keep_header(name, excluded, &connection_tokens))
        .map(|(name, value)| (name.as_str().to_owned(), value.as_bytes().to_vec()))
        .collect()
}

fn selected_headers(headers: &HeaderMap, excluded: &[&str]) -> Vec<(String, String)> {
    let connection_tokens = parse_connection_tokens(headers);
    headers
        .iter()
        .filter(|(name, _)| should_keep_header(name, excluded, &connection_tokens))
        .map(|(name, value)| (name.as_str().to_owned(), header_value_string(value)))
        .collect()
}

fn header_value_string(value: &axum::http::HeaderValue) -> String {
    value.to_str().map_or_else(
        |_| percent_encode(value.as_bytes(), NON_ALPHANUMERIC).to_string(),
        ToOwned::to_owned,
    )
}

fn should_keep_header(
    name: &HeaderName,
    excluded: &[&str],
    connection_tokens: &HashSet<String>,
) -> bool {
    let name_text = name.as_str();
    !connection_tokens.contains(&name_text.to_ascii_lowercase())
        && !HOP_BY_HOP_HEADERS
            .iter()
            .chain(excluded.iter())
            .any(|candidate| name_text.eq_ignore_ascii_case(candidate))
}

fn parse_connection_tokens(headers: &HeaderMap) -> HashSet<String> {
    headers
        .get_all(axum::http::header::CONNECTION)
        .iter()
        .flat_map(|value| value.as_bytes().split(|byte| *byte == b','))
        .map(lowercase_trimmed_ascii_token)
        .filter(|token| !token.is_empty())
        .collect()
}

fn lowercase_trimmed_ascii_token(token: &[u8]) -> String {
    let start = token
        .iter()
        .position(|byte| !byte.is_ascii_whitespace())
        .unwrap_or(token.len());
    let end = token
        .iter()
        .rposition(|byte| !byte.is_ascii_whitespace())
        .map_or(start, |position| position + 1);
    let normalized = token
        .get(start..end)
        .unwrap_or_default()
        .iter()
        .map(u8::to_ascii_lowercase)
        .collect::<Vec<_>>();
    String::from_utf8_lossy(&normalized).into_owned()
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

    #[rstest]
    fn selected_response_headers_preserve_non_utf8_values() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-raw",
            axum::http::HeaderValue::from_bytes(b"\xff\xfe").expect("valid raw header"),
        );

        assert_eq!(
            selected_response_headers(&headers),
            vec![("x-raw".to_owned(), "%FF%FE".to_owned())],
        );
    }

    #[rstest]
    fn selected_response_proxy_headers_preserve_raw_values() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-raw",
            axum::http::HeaderValue::from_bytes(b"\xff\xfe").expect("valid raw header"),
        );

        assert_eq!(
            selected_response_proxy_headers(&headers),
            vec![("x-raw".to_owned(), b"\xff\xfe".to_vec())],
        );
    }

    #[rstest]
    fn selected_headers_drop_connection_token_headers() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "connection",
            "keep-alive, x-hop".parse().expect("valid header"),
        );
        headers.insert("x-hop", "drop-me".parse().expect("valid header"));
        headers.insert(
            "content-type",
            "application/json".parse().expect("valid header"),
        );

        assert_eq!(
            selected_request_headers(&headers),
            vec![("content-type".to_owned(), "application/json".to_owned())],
        );
    }

    #[rstest]
    fn selected_headers_parse_connection_tokens_from_raw_bytes() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "connection",
            axum::http::HeaderValue::from_bytes(b" x-hop , \xff")
                .expect("valid opaque header bytes"),
        );
        headers.insert("x-hop", "drop-me".parse().expect("valid header"));
        headers.insert(
            "content-type",
            "application/json".parse().expect("valid header"),
        );

        assert_eq!(
            selected_request_headers(&headers),
            vec![("content-type".to_owned(), "application/json".to_owned())],
        );
    }

    #[rstest]
    fn selected_forward_headers_preserve_raw_values() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-raw",
            axum::http::HeaderValue::from_bytes(b"\xff\xfe").expect("valid raw header"),
        );

        assert_eq!(
            selected_forward_headers(&headers),
            vec![("x-raw".to_owned(), b"\xff\xfe".to_vec())],
        );
    }
}
