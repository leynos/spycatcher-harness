//! Unit tests for header filtering and JSON parsing helpers.

use super::*;
use rstest::rstest;

use crate::config::RedactionConfig;

/// Build a `HeaderMap` from raw byte pairs.
fn make_header_map(entries: &[(&str, &[u8])]) -> HeaderMap {
    let mut map = HeaderMap::new();
    for (name, value) in entries {
        let header_name = match axum::http::HeaderName::from_bytes(name.as_bytes()) {
            Ok(header_name) => header_name,
            Err(error) => panic!("valid header name: {error}"),
        };
        let header_value = match axum::http::HeaderValue::from_bytes(value) {
            Ok(header_value) => header_value,
            Err(error) => panic!("valid header value bytes: {error}"),
        };
        map.insert(header_name, header_value);
    }
    map
}

/// Protects capture from connection and destination headers while retaining
/// application headers.
#[rstest]
fn selected_request_headers_drop_hop_by_hop_and_framing_headers() {
    let headers = make_header_map(&[
        ("content-type", b"application/json"),
        ("host", b"example.invalid"),
        ("connection", b"keep-alive"),
        ("authorization", b"Bearer keep-me"),
    ]);

    assert_eq!(
        selected_request_headers(&headers),
        vec![
            ("content-type".to_owned(), "application/json".to_owned()),
            ("authorization".to_owned(), "Bearer keep-me".to_owned()),
        ],
    );
}

/// Prevents differently cased credential names from bypassing redaction while
/// retaining trace headers.
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

/// Keeps malformed body bytes from being mistaken for successfully parsed JSON.
#[rstest]
fn parse_json_bytes_returns_none_for_invalid_json() {
    assert_eq!(parse_json_bytes(br#"{"unterminated": true"#), None);
}

/// Preserves opaque upstream header values in a stable textual cassette
/// representation.
#[rstest]
fn selected_response_headers_preserve_non_utf8_values() {
    let headers = make_header_map(&[("x-raw", b"\xff\xfe")]);

    assert_eq!(
        selected_response_headers(&headers),
        vec![("x-raw".to_owned(), "%FF%FE".to_owned())],
    );
}

/// Keeps opaque upstream header bytes unchanged at the downstream proxy
/// boundary.
#[rstest]
fn selected_response_proxy_headers_preserve_raw_values() {
    let headers = make_header_map(&[("x-raw", b"\xff\xfe")]);

    assert_eq!(
        selected_response_proxy_headers(&headers),
        vec![("x-raw".to_owned(), b"\xff\xfe".to_vec())],
    );
}

/// Enforces dynamic Connection exclusions despite a neighbouring non-UTF-8
/// token.
#[rstest]
#[case(
    b"keep-alive, x-hop" as &[u8],
    "UTF-8 connection tokens are parsed and applied"
)]
#[case(
    b" x-hop , \xff" as &[u8],
    "non-UTF-8 bytes in Connection header do not suppress valid tokens"
)]
fn selected_headers_drop_connection_token_headers(
    #[case] connection_value: &[u8],
    #[case] description: &str,
) {
    let headers = make_header_map(&[
        ("connection", connection_value),
        ("x-hop", b"drop-me"),
        ("content-type", b"application/json"),
    ]);

    assert_eq!(
        selected_request_headers(&headers),
        vec![("content-type".to_owned(), "application/json".to_owned())],
        "{description}",
    );
}

/// Keeps opaque inbound header bytes unchanged at the upstream forwarding
/// boundary.
#[rstest]
fn selected_forward_headers_preserve_raw_values() {
    let headers = make_header_map(&[("x-raw", b"\xff\xfe")]);

    assert_eq!(
        selected_forward_headers(&headers),
        vec![("x-raw".to_owned(), b"\xff\xfe".to_vec())],
    );
}

mod prop_tests {
    //! Property tests for header selection invariants.

    use super::*;
    use proptest::prelude::*;

    proptest! {
        /// Checks every hop-by-hop name stays excluded across generated legal
        /// header values.
        #[test]
        fn hop_by_hop_headers_are_always_dropped(value in "[a-zA-Z0-9]{1,16}") {
            for name in HOP_BY_HOP_HEADERS {
                let mut map = axum::http::HeaderMap::new();
                map.insert(
                    axum::http::HeaderName::from_bytes(name.as_bytes())
                        .expect("hop-by-hop header name should be valid"),
                    axum::http::HeaderValue::from_str(&value)
                        .expect("generated header value should be valid"),
                );
                let result = selected_request_headers(&map);
                prop_assert!(
                    result.iter().all(|(n, _)| !n.eq_ignore_ascii_case(name)),
                    "hop-by-hop header {name:?} must be dropped"
                );
            }
        }
    }

    proptest! {
        /// Protects generated opaque header values from being silently
        /// discarded.
        #[test]
        fn non_utf8_bytes_are_percent_encoded_not_dropped(
            suffix in proptest::collection::vec(0x80u8..=0xFFu8, 1..8),
        ) {
            let mut map = axum::http::HeaderMap::new();
            map.insert(
                axum::http::HeaderName::from_bytes(b"x-custom")
                    .expect("custom header name should be valid"),
                axum::http::HeaderValue::from_bytes(&suffix)
                    .expect("generated non-UTF-8 header value should be valid"),
            );
            let result = selected_request_headers(&map);
            prop_assert!(!result.is_empty(), "non-UTF-8 header must not be dropped");
            let (_, value) = result
                .first()
                .expect("selected headers should contain x-custom");
            let expected = percent_encode(&suffix, NON_ALPHANUMERIC).to_string();
            prop_assert_eq!(value, &expected);
        }
    }
}
