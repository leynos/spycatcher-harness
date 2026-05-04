//! Outbound upstream HTTP client adapters and environment access ports.
//!
//! Record mode uses these helpers to enrich outbound requests with
//! authentication and forward them to the configured provider without leaking
//! client-library types into cassette logic.

use axum::http::{HeaderName, HeaderValue};
use reqwest::{Client, Url};

use crate::config::UpstreamConfig;
use crate::http_exchange::{ObservedResponse, parse_json_bytes, selected_response_headers};
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
    /// Selected inbound request headers to forward upstream.
    pub headers: &'a [(String, String)],
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
        Ok(Self { client })
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
                HeaderValue::from_str(value).map_err(|error| HarnessError::InvalidConfig {
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
}
