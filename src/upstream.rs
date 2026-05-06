//! Outbound upstream HTTP client adapters and environment access ports.
//!
//! Record mode uses these helpers to enrich outbound requests with
//! authentication and forward them to the configured provider without leaking
//! client-library types into cassette logic.

use axum::http::{HeaderName, HeaderValue};
use reqwest::{Client, Url};

use crate::config::UpstreamConfig;
use crate::http_exchange::{
    ObservedResponse, parse_json_bytes, selected_response_headers, selected_response_proxy_headers,
};
use crate::{HarnessError, HarnessResult};
use std::time::Duration;

/// Request timeout applied to the Reqwest client.
///
/// Thirty seconds bounds non-stream LLM completions without allowing
/// indefinite upstream hangs.
pub(crate) const UPSTREAM_TIMEOUT: Duration = Duration::from_secs(30);

/// Narrow environment lookup port used by record-mode request handling.
pub(crate) trait EnvProvider {
    /// Returns the value of an environment variable when it is present.
    fn read(&self, name: &str) -> Option<String>;
}

/// Production environment lookup using process state.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct ProcessEnvProvider;

impl EnvProvider for ProcessEnvProvider {
    fn read(&self, name: &str) -> Option<String> {
        std::env::var(name).ok()
    }
}

/// Narrow outbound port for chat completions proxy requests.
pub(crate) trait ChatCompletionsUpstream {
    /// Forwards one chat completions request to the configured upstream.
    async fn send_chat_completions(
        &self,
        request: ChatCompletionsRequest<'_>,
    ) -> HarnessResult<ObservedResponse>;
}

/// Outbound request data for one chat completions proxy call.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ChatCompletionsRequest<'a> {
    /// Provider configuration for the upstream exchange.
    pub config: &'a UpstreamConfig,
    /// Bearer token resolved from the configured environment variable.
    pub api_key: &'a str,
    /// Selected inbound request headers to forward upstream as raw bytes.
    pub headers: &'a [(String, Vec<u8>)],
    /// Exact inbound request body bytes.
    pub body: &'a [u8],
    /// Raw query string from the inbound request.
    pub query: &'a str,
}

/// Reqwest-backed upstream adapter for record mode.
#[derive(Debug, Clone)]
pub(crate) struct ReqwestUpstreamClient {
    client: Client,
}

impl ReqwestUpstreamClient {
    /// Creates an upstream client that preserves response bytes as received.
    ///
    /// # Errors
    ///
    /// Returns [`HarnessError::InvalidConfig`] when the client cannot be
    /// constructed.
    pub(crate) fn new() -> HarnessResult<Self> {
        let client = Client::builder()
            .no_gzip()
            .no_brotli()
            .no_deflate()
            .no_zstd()
            .timeout(UPSTREAM_TIMEOUT)
            .build()
            .map_err(|error| HarnessError::InvalidConfig {
                message: format!("failed to construct upstream client: {error}"),
            })?;
        Ok(Self::with_client(client))
    }

    /// Creates an upstream client with a custom pre-built reqwest client.
    /// Intended for tests that need to control timeout or TLS behaviour.
    pub(crate) const fn with_client(client: Client) -> Self {
        Self { client }
    }
}

impl ChatCompletionsUpstream for ReqwestUpstreamClient {
    async fn send_chat_completions(
        &self,
        request: ChatCompletionsRequest<'_>,
    ) -> HarnessResult<ObservedResponse> {
        let url = chat_completions_url(&request.config.base_url, request.query)?;
        let mut outbound = self.client.post(url).bearer_auth(request.api_key);

        for (name, value) in request.headers {
            if name.eq_ignore_ascii_case("authorization") {
                continue;
            }
            let header_name = HeaderName::try_from(name.as_str()).map_err(|error| {
                HarnessError::InvalidConfig {
                    message: format!("invalid outbound header name {name:?}: {error}"),
                }
            })?;
            let header_value =
                HeaderValue::from_bytes(value).map_err(|error| HarnessError::InvalidConfig {
                    message: format!("invalid outbound header value for {name:?}: {error}"),
                })?;
            outbound = outbound.header(header_name, header_value);
        }

        for (name, value) in &request.config.extra_headers {
            outbound = outbound.header(name, value);
        }

        let response = outbound
            .body(request.body.to_vec())
            .send()
            .await
            .map_err(|source| HarnessError::UpstreamRequestFailed {
                source: source.into(),
            })?;
        let status = response.status().as_u16();
        let selected_headers = selected_response_headers(response.headers());
        let proxy_headers = selected_response_proxy_headers(response.headers());
        let response_body = response
            .bytes()
            .await
            .map_err(|source| HarnessError::UpstreamRequestFailed {
                source: source.into(),
            })?
            .to_vec();

        Ok(ObservedResponse {
            status,
            headers: selected_headers,
            proxy_headers,
            parsed_json: parse_json_bytes(&response_body),
            body: response_body,
        })
    }
}

/// Builds the upstream chat completions URL from the configured base URL and
/// optional query string.
///
/// # Errors
///
/// Returns [`HarnessError::InvalidConfig`] when the base URL is not a valid
/// absolute URL.
pub(crate) fn chat_completions_url(base_url: &str, query: &str) -> HarnessResult<Url> {
    let mut url = Url::parse(base_url).map_err(|error| HarnessError::InvalidConfig {
        message: format!("invalid upstream base URL {base_url:?}: {error}"),
    })?;
    {
        let mut segments = url
            .path_segments_mut()
            .map_err(|()| HarnessError::InvalidConfig {
                message: format!("upstream base URL {base_url:?} cannot be a base URL"),
            })?;
        segments.pop_if_empty();
        segments.push("chat");
        segments.push("completions");
    }
    if !query.is_empty() {
        url.set_query(Some(query));
    }
    Ok(url)
}

#[cfg(test)]
mod tests {
    //! Unit tests for upstream URL construction.

    use rstest::rstest;

    use super::*;

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
    fn chat_completions_url_appends_endpoint_path(
        #[case] base_url: &str,
        #[case] query: &str,
        #[case] expected: &str,
    ) {
        let actual = chat_completions_url(base_url, query).expect("base URL should parse");
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
    #[tokio::test]
    async fn send_chat_completions_uses_bearer_auth_and_skips_inbound_authorization() {
        use std::io::Read;
        use std::net::TcpListener;
        use std::sync::{Arc, Mutex};

        let listener = TcpListener::bind("127.0.0.1:0").expect("listener should bind");
        let addr = listener
            .local_addr()
            .expect("listener address should be available");
        let captured = Arc::new(Mutex::new(Vec::new()));
        let captured_clone = Arc::clone(&captured);

        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("server should accept one request");
            let mut buf = vec![0_u8; 4096];
            let n = stream.read(&mut buf).unwrap_or(0);
            buf.truncate(n);
            *captured_clone
                .lock()
                .expect("captured request lock should not be poisoned") = buf;
        });

        let client = Client::builder()
            .timeout(Duration::from_millis(500))
            .build()
            .expect("custom reqwest client should build");
        let upstream = ReqwestUpstreamClient::with_client(client);

        let config = UpstreamConfig {
            base_url: format!("http://{addr}"),
            ..UpstreamConfig::default()
        };
        let headers = vec![
            (
                "authorization".to_owned(),
                b"Bearer downstream-secret".to_vec(),
            ),
            ("x-custom".to_owned(), b"keep-me".to_vec()),
        ];

        drop(
            upstream
                .send_chat_completions(ChatCompletionsRequest {
                    config: &config,
                    api_key: "upstream-key",
                    headers: &headers,
                    body: br#"{"model":"test"}"#,
                    query: "",
                })
                .await,
        );

        std::thread::sleep(Duration::from_millis(200));

        let raw = captured
            .lock()
            .expect("captured request lock should not be poisoned");
        let raw_str = String::from_utf8_lossy(&raw);
        let raw_lower = raw_str.to_ascii_lowercase();

        assert!(
            raw_lower.contains("authorization: bearer upstream-key"),
            "upstream request must carry configured Bearer token; got:\n{raw_str}",
        );
        assert!(
            !raw_str.contains("downstream-secret"),
            "downstream Authorization must not be forwarded; got:\n{raw_str}",
        );
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
                let base = "https://example.invalid/v1";
                let url = chat_completions_url(base, &query).expect("URL must build");
                if query.is_empty() {
                    prop_assert!(!url.as_str().contains('?'));
                } else {
                    prop_assert!(url.as_str().contains(&query));
                }
            }
        }
    }
}
