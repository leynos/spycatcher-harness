//! Adapter-neutral HTTP exchange types and header-handling helpers.
//!
//! This module keeps request and response capture policy in one place so the
//! server and upstream adapters agree on what gets forwarded, returned, and
//! persisted.

use std::collections::HashSet;
use std::fmt;

use axum::http::HeaderMap;
use futures_util::stream::BoxStream;
use percent_encoding::{NON_ALPHANUMERIC, percent_encode};
use serde_json::Value;

use crate::HarnessResult;
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
pub(crate) struct ProxyResponse {
    /// HTTP status code.
    pub status: u16,
    /// Selected headers as raw bytes in observed order.
    pub headers: Vec<(String, Vec<u8>)>,
    /// Response body for downstream proxying.
    pub body: ProxyBody,
}

impl fmt::Debug for ProxyResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let header_names = self
            .headers
            .iter()
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>();
        formatter
            .debug_struct("ProxyResponse")
            .field("status", &self.status)
            .field("headers", &header_names)
            .field("body", &self.body)
            .finish()
    }
}

/// Body data returned to the inbound HTTP adapter.
pub(crate) enum ProxyBody {
    /// A fully buffered response body.
    Buffered(Vec<u8>),
    /// A streaming response body.
    Stream(BoxStream<'static, HarnessResult<axum::body::Bytes>>),
}

impl fmt::Debug for ProxyBody {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Buffered(bytes) => formatter
                .debug_tuple("Buffered")
                .field(&bytes.len())
                .finish(),
            Self::Stream(_) => formatter.write_str("Stream"),
        }
    }
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
    select_headers_unified(headers, excluded, |value| value.as_bytes().to_vec())
}

fn selected_headers(headers: &HeaderMap, excluded: &[&str]) -> Vec<(String, String)> {
    select_headers_unified(headers, excluded, header_value_string)
}

fn select_headers_unified<V>(
    headers: &HeaderMap,
    excluded: &[&str],
    map_value: impl Fn(&axum::http::HeaderValue) -> V,
) -> Vec<(String, V)> {
    let disallowed = build_disallowed_set(headers, excluded);
    headers
        .iter()
        .filter(|(name, _)| !disallowed.contains(name.as_str()))
        .map(|(name, value)| (name.as_str().to_owned(), map_value(value)))
        .collect()
}

fn header_value_string(value: &axum::http::HeaderValue) -> String {
    value.to_str().map_or_else(
        |_| percent_encode(value.as_bytes(), NON_ALPHANUMERIC).to_string(),
        ToOwned::to_owned,
    )
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

fn build_disallowed_set(headers: &HeaderMap, excluded: &[&str]) -> HashSet<String> {
    let mut set = parse_connection_tokens(headers);
    for name in HOP_BY_HOP_HEADERS.iter().chain(excluded.iter()) {
        set.insert(name.to_ascii_lowercase());
    }
    set
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
#[path = "http_exchange_debug_tests.rs"]
mod debug_tests;

#[cfg(test)]
#[path = "http_exchange_tests.rs"]
mod tests;
