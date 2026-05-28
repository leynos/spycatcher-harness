//! Unit tests for upstream URL construction and request building.

use super::*;
use rstest::rstest;

#[rstest]
#[case(
    "https://openrouter.ai/api/v1",
    "",
    "https://openrouter.ai/api/v1/chat/completions"
)]
#[case(
    "https://openrouter.ai/api/v1/",
    "",
    "https://openrouter.ai/api/v1/chat/completions"
)]
#[case(
    "https://openrouter.ai/api/v1",
    "foo=bar&baz=1",
    "https://openrouter.ai/api/v1/chat/completions?foo=bar&baz=1"
)]
#[case(
    "https://openrouter.ai/api/v1?configured=true",
    "foo=bar",
    "https://openrouter.ai/api/v1/chat/completions?configured=true&foo=bar"
)]
fn chat_completions_url_appends_endpoint_path(
    #[case] base_url: &str,
    #[case] query: &str,
    #[case] expected: &str,
) {
    let parsed = test_url(base_url);
    let actual = chat_completions_url(&parsed, query).expect("URL construction must succeed");
    assert_eq!(actual.as_str(), expected);
}

#[rstest]
fn reqwest_upstream_client_accepts_injected_client() {
    let client = Client::builder()
        .timeout(Duration::from_millis(1))
        .build()
        .expect("custom reqwest client should build");

    drop(ReqwestUpstreamClient::with_client(client));
}

#[rstest]
fn to_outbound_header_accepts_valid_utf8_bytes() {
    let (name, value) =
        to_outbound_header("content-type", b"application/json").expect("valid header");
    assert_eq!(name.as_str(), "content-type");
    assert_eq!(value.as_bytes(), b"application/json");
}

#[rstest]
fn to_outbound_header_accepts_non_utf8_bytes() {
    let (_, value) =
        to_outbound_header("x-raw", b"\xff\xfe").expect("valid non-UTF-8 header value");
    assert_eq!(value.as_bytes(), b"\xff\xfe");
}

#[derive(Clone, Copy)]
enum InjectCase {
    Forwarded,
    Extra,
}

#[rstest]
#[case(InjectCase::Forwarded, "x-custom: keep-me")]
#[case(InjectCase::Extra, "x-provider-id: acme")]
#[tokio::test]
async fn headers_skip_authorization_and_forward_expected(
    #[case] case: InjectCase,
    #[case] expected_header: &str,
) {
    let (addr, captured, server) = spawn_capturing_server();
    let base_builder = Client::builder()
        .timeout(Duration::from_millis(500))
        .build()
        .expect("custom reqwest client should build")
        .post(format!("http://{addr}"));
    let request_builder = match case {
        InjectCase::Forwarded => {
            let headers = vec![
                (
                    "authorization".to_owned(),
                    b"Bearer should-not-forward".to_vec(),
                ),
                ("x-custom".to_owned(), b"keep-me".to_vec()),
            ];
            apply_forwarded_headers(base_builder, &headers).expect("forwarded headers should apply")
        }
        InjectCase::Extra => {
            let mut extra = std::collections::BTreeMap::new();
            extra.insert("Authorization".to_owned(), "Bearer extra-secret".to_owned());
            extra.insert("x-provider-id".to_owned(), "acme".to_owned());
            apply_extra_headers(base_builder, &extra).expect("extra headers should apply")
        }
    };

    request_builder
        .body("{}".to_owned())
        .send()
        .await
        .expect("outbound request should succeed");

    server
        .join()
        .expect("server thread should not panic")
        .expect("server should capture one request");
    let raw_str = wait_and_collect(&captured).expect("captured request should be readable");
    let raw_lower = raw_str.to_ascii_lowercase();
    assert!(
        !raw_lower.contains("authorization:"),
        "must drop Authorization; got:\n{raw_str}",
    );
    assert!(
        raw_str.contains(expected_header),
        "must forward expected non-auth header {expected_header:?}; got:\n{raw_str}",
    );
}

#[rstest]
fn apply_extra_headers_rejects_invalid_header_name() {
    let builder = Client::new().post("http://example.invalid/");
    let mut extra = std::collections::BTreeMap::new();
    extra.insert("not a header".to_owned(), "value".to_owned());

    assert!(matches!(
        apply_extra_headers(builder, &extra),
        Err(HarnessError::InvalidConfig { .. })
    ));
}

#[rstest]
fn apply_extra_headers_rejects_forbidden_header_name() {
    let builder = Client::new().post("http://example.invalid/");
    let mut extra = std::collections::BTreeMap::new();
    extra.insert("connection".to_owned(), "keep-alive".to_owned());

    assert!(matches!(
        apply_extra_headers(builder, &extra),
        Err(HarnessError::InvalidConfig { .. })
    ));
}

type CapturedRequest = std::sync::Arc<std::sync::Mutex<Vec<u8>>>;
type CaptureThread = std::thread::JoinHandle<Result<(), String>>;

fn spawn_capturing_server() -> (std::net::SocketAddr, CapturedRequest, CaptureThread) {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    let listener = match TcpListener::bind("127.0.0.1:0") {
        Ok(listener) => listener,
        Err(error) => panic!("listener should bind: {error}"),
    };
    let addr = match listener.local_addr() {
        Ok(addr) => addr,
        Err(error) => panic!("listener address should be available: {error}"),
    };
    let captured: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
    let captured_clone = Arc::clone(&captured);

    let handle = std::thread::spawn(move || {
        let (mut stream, _) = listener
            .accept()
            .map_err(|error| format!("server should accept one request: {error}"))?;
        let mut buf = Vec::new();
        let mut chunk = [0_u8; 1024];
        loop {
            let n = stream
                .read(&mut chunk)
                .map_err(|error| format!("server should read one request: {error}"))?;
            if n == 0 {
                break;
            }
            let read_bytes = chunk
                .get(..n)
                .ok_or_else(|| format!("server read {n} bytes beyond buffer bounds"))?;
            buf.extend_from_slice(read_bytes);
            if buf
                .windows(b"\r\n\r\n".len())
                .any(|window| window == b"\r\n\r\n")
            {
                break;
            }
        }
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n")
            .map_err(|error| format!("server should write response: {error}"))?;
        let mut captured_guard = captured_clone
            .lock()
            .map_err(|error| format!("captured request lock should not be poisoned: {error}"))?;
        *captured_guard = buf;
        Ok(())
    });

    (addr, captured, handle)
}

fn auth_test_config(addr: std::net::SocketAddr) -> UpstreamConfig {
    let base_url = test_url(&format!("http://{addr}"));
    UpstreamConfig {
        base_url,
        extra_headers: [("Authorization".to_owned(), "Bearer extra-secret".to_owned())].into(),
        ..UpstreamConfig::default()
    }
}

fn inbound_auth_test_headers() -> Vec<(String, Vec<u8>)> {
    vec![
        (
            "authorization".to_owned(),
            b"Bearer downstream-secret".to_vec(),
        ),
        ("x-custom".to_owned(), b"keep-me".to_vec()),
    ]
}

fn wait_and_collect(captured: &CapturedRequest) -> Result<String, String> {
    std::thread::sleep(Duration::from_millis(200));
    let raw = captured
        .lock()
        .map_err(|error| format!("captured request lock should not be poisoned: {error}"))?;
    Ok(String::from_utf8_lossy(&raw).into_owned())
}

#[rstest]
#[tokio::test]
async fn send_chat_completions_uses_bearer_auth_and_skips_inbound_authorization() {
    let (addr, captured, server) = spawn_capturing_server();

    let client = Client::builder()
        .timeout(Duration::from_millis(500))
        .build()
        .expect("custom reqwest client should build");
    let upstream = ReqwestUpstreamClient::with_client(client);
    let config = auth_test_config(addr);
    let headers = inbound_auth_test_headers();

    upstream
        .send_chat_completions(ChatCompletionsRequest {
            config: &config,
            api_key: "upstream-key",
            headers: &headers,
            body: br#"{"model":"test"}"#,
            query: "",
        })
        .await
        .expect("outbound request should succeed");

    server
        .join()
        .expect("server thread should not panic")
        .expect("server should capture one request");
    let raw_str = wait_and_collect(&captured).expect("captured request should be readable");
    let raw_lower = raw_str.to_ascii_lowercase();

    assert!(
        raw_lower.contains("authorization: bearer upstream-key"),
        "upstream request must carry configured Bearer token; got:\n{raw_str}",
    );
    assert!(!raw_str.contains("downstream-secret"));
    assert!(!raw_str.contains("extra-secret"));
    assert!(
        raw_str.contains("x-custom: keep-me"),
        "non-Authorization header must be forwarded; got:\n{raw_str}",
    );
}

mod prop_tests {
    //! Property tests for upstream URL construction.

    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn url_query_string_is_preserved(query in "[a-z0-9=&]{0,40}") {
            let base = test_url("https://example.invalid/v1");
            let url = chat_completions_url(&base, &query).expect("URL must build");
            if query.is_empty() {
                prop_assert!(url.query().is_none());
            } else {
                prop_assert_eq!(url.query(), Some(query.as_str()));
            }
        }
    }
}

fn test_url(value: &str) -> Url {
    match Url::parse(value) {
        Ok(url) => url,
        Err(error) => panic!("test fixture URL is invalid: {error}"),
    }
}
