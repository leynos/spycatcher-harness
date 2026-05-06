//! Unit tests for shared server adapter helpers.

use rstest::rstest;

use super::record_handler::log_chat_request;

#[rstest]
#[tracing_test::traced_test]
fn log_chat_request_uses_path_not_full_uri() {
    use axum::http::Uri;
    let uri: Uri = "/v1/chat/completions?api_key=secret"
        .parse()
        .expect("URI should parse");
    log_chat_request(&axum::http::Method::POST, &uri);
    assert!(
        logs_contain("/v1/chat/completions"),
        "logged output should contain the path"
    );
    assert!(
        !logs_contain("api_key=secret"),
        "logged output must not contain the query string"
    );
}
