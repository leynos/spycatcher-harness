//! Outbound upstream HTTP client adapters and environment access ports.
//!
//! Record mode uses these helpers to enrich outbound requests with
//! authentication and forward them to the configured provider without leaking
//! client-library types into cassette logic.

use axum::http::{HeaderName, HeaderValue};
use futures_util::StreamExt;
use futures_util::stream::BoxStream;
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

const FORBIDDEN_EXTRA_HEADERS: &[&str] = &[
    "host",
    "connection",
    "keep-alive",
    "proxy-authenticate",
    "proxy-authorization",
    "te",
    "trailer",
    "transfer-encoding",
    "upgrade",
    "content-length",
    "accept-encoding",
];

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

    /// Forwards one streaming chat completions request to the configured
    /// upstream.
    async fn stream_chat_completions(
        &self,
        request: ChatCompletionsRequest<'_>,
    ) -> HarnessResult<StreamingObservedResponse>;
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

/// Captured metadata and byte stream for one upstream streaming response.
pub(crate) struct StreamingObservedResponse {
    /// HTTP status code.
    pub status: u16,
    /// Selected headers in observed order for cassette persistence.
    pub headers: Vec<(String, String)>,
    /// Selected headers as raw bytes for downstream proxying.
    pub proxy_headers: Vec<(String, Vec<u8>)>,
    /// Upstream response body bytes as received from reqwest.
    pub body: BoxStream<'static, HarnessResult<axum::body::Bytes>>,
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

fn to_outbound_header(name: &str, value: &[u8]) -> HarnessResult<(HeaderName, HeaderValue)> {
    let header_name = HeaderName::try_from(name).map_err(|error| HarnessError::InvalidConfig {
        message: format!("invalid outbound header name {name:?}: {error}"),
    })?;
    let header_value =
        HeaderValue::from_bytes(value).map_err(|error| HarnessError::InvalidConfig {
            message: format!("invalid outbound header value for {name:?}: {error}"),
        })?;
    Ok((header_name, header_value))
}

#[inline]
fn should_forward_header(name: &str) -> bool {
    !name.eq_ignore_ascii_case("authorization")
}

fn is_forbidden_extra_header(name: &str) -> bool {
    FORBIDDEN_EXTRA_HEADERS
        .iter()
        .any(|forbidden| forbidden.eq_ignore_ascii_case(name))
}

fn apply_forwarded_headers(
    mut builder: reqwest::RequestBuilder,
    headers: &[(String, Vec<u8>)],
) -> HarnessResult<reqwest::RequestBuilder> {
    for (name, value) in headers {
        if !should_forward_header(name) {
            continue;
        }
        let (header_name, header_value) = to_outbound_header(name, value)?;
        builder = builder.header(header_name, header_value);
    }
    Ok(builder)
}

fn apply_extra_headers(
    mut builder: reqwest::RequestBuilder,
    extra_headers: &std::collections::BTreeMap<String, String>,
) -> HarnessResult<reqwest::RequestBuilder> {
    for (name, value) in extra_headers {
        if !should_forward_header(name) {
            continue;
        }
        if is_forbidden_extra_header(name) {
            return Err(HarnessError::InvalidConfig {
                message: format!("extra header {name:?} is not allowed"),
            });
        }
        let (header_name, header_value) = to_outbound_header(name, value.as_bytes())?;
        builder = builder.header(header_name, header_value);
    }
    Ok(builder)
}

impl ChatCompletionsUpstream for ReqwestUpstreamClient {
    async fn send_chat_completions(
        &self,
        request: ChatCompletionsRequest<'_>,
    ) -> HarnessResult<ObservedResponse> {
        let url = chat_completions_url(&request.config.base_url, request.query)?;
        let authed_builder = self.client.post(url).bearer_auth(request.api_key);
        let forwarded_builder = apply_forwarded_headers(authed_builder, request.headers)?;
        let extra_builder = apply_extra_headers(forwarded_builder, &request.config.extra_headers)?;

        let response = extra_builder
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

    async fn stream_chat_completions(
        &self,
        request: ChatCompletionsRequest<'_>,
    ) -> HarnessResult<StreamingObservedResponse> {
        let url = chat_completions_url(&request.config.base_url, request.query)?;
        let authed_builder = self.client.post(url).bearer_auth(request.api_key);
        let forwarded_builder = apply_forwarded_headers(authed_builder, request.headers)?;
        let extra_builder = apply_extra_headers(forwarded_builder, &request.config.extra_headers)?;

        let upstream_response = extra_builder
            .body(request.body.to_vec())
            .send()
            .await
            .map_err(|source| HarnessError::UpstreamRequestFailed {
                source: source.into(),
            })?;
        let status = upstream_response.status().as_u16();
        let selected_headers = selected_response_headers(upstream_response.headers());
        let proxy_headers = selected_response_proxy_headers(upstream_response.headers());
        let body = futures_util::stream::try_unfold(upstream_response, |mut streamed| async move {
            streamed
                .chunk()
                .await
                .map(|maybe_chunk| maybe_chunk.map(|chunk| (chunk, streamed)))
                .map_err(|source| HarnessError::UpstreamRequestFailed {
                    source: source.into(),
                })
        })
        .boxed();

        Ok(StreamingObservedResponse {
            status,
            headers: selected_headers,
            proxy_headers,
            body,
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
        match url.query() {
            Some(existing) => {
                let merged_query = format!("{existing}&{query}");
                url.set_query(Some(&merged_query));
            }
            None => url.set_query(Some(query)),
        }
    }
    Ok(url)
}

#[cfg(test)]
#[path = "upstream_tests.rs"]
mod tests;
