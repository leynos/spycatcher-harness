//! Step definitions for record-mode proxying scenarios.

use std::error::Error;
use std::net::SocketAddr;
use std::sync::Arc;

use rstest_bdd_macros::{given, then, when};

use spycatcher_harness::config::{ListenAddr, Mode, RedactionConfig, UpstreamConfig, UpstreamKind};
use spycatcher_harness::{HarnessConfig, start_harness};

use crate::record_mode_proxying::helpers::{
    CapturedRequest, StubUpstream, assert_cassette_matches_success_snapshot,
    assert_upstream_bearer_token, load_cassette, present_env_name, sample_success_body,
    send_request, unique_cassette_path,
};
use crate::record_mode_proxying::world::ProxyWorld;

const NON_STREAM_REQUEST: &[u8] =
    br#"{"model":"gpt-test","messages":[{"role":"user","content":"hi"}]}"#;
pub(crate) const STREAMING_REQUEST: &[u8] =
    br#"{"model":"gpt-test","stream":true,"messages":[{"role":"user","content":"hi"}]}"#;

#[given("a stub upstream that returns a successful chat completion")]
fn a_stub_upstream_that_returns_a_successful_chat_completion(
    proxy_world: &ProxyWorld,
) -> Result<(), Box<dyn Error>> {
    let upstream = proxy_world
        .runtime
        .with_ref(|runtime| StubUpstream::start(runtime, sample_success_body()))
        .ok_or_else(|| std::io::Error::other("runtime must be set"))??;
    proxy_world.upstream.set(upstream);
    Ok(())
}

#[given("a record-mode harness configured for that upstream")]
fn a_record_mode_harness_configured_for_that_upstream(
    proxy_world: &ProxyWorld,
) -> Result<(), Box<dyn Error>> {
    configure_harness_with_proxy_world_upstream(proxy_world, "success", RedactionConfig::default())
}

#[given("a record-mode harness configured for that upstream with header redaction")]
fn a_record_mode_harness_configured_for_that_upstream_with_header_redaction(
    proxy_world: &ProxyWorld,
) -> Result<(), Box<dyn Error>> {
    configure_harness_with_proxy_world_upstream(
        proxy_world,
        "redaction",
        RedactionConfig {
            drop_headers: vec!["x-session-secret".to_owned()],
        },
    )
}

#[given("a record-mode harness configured with an unavailable upstream")]
fn a_record_mode_harness_configured_with_an_unavailable_upstream(
    proxy_world: &ProxyWorld,
) -> Result<(), Box<dyn Error>> {
    let cassette_path = unique_cassette_path("failure");
    proxy_world.cassette_path.set(cassette_path.clone());
    proxy_world.config.set(make_record_config(
        &cassette_path,
        UpstreamConfig {
            kind: UpstreamKind::OpenRouter,
            base_url: "http://127.0.0.1:1/api/v1".to_owned(),
            api_key_env: present_env_name()?.to_owned(),
            extra_headers: std::collections::BTreeMap::new(),
        },
        RedactionConfig::default(),
    )?);
    Ok(())
}

#[when("the harness is started")]
fn the_harness_is_started(proxy_world: &ProxyWorld) -> Result<(), Box<dyn Error>> {
    let config = proxy_world
        .config
        .take()
        .ok_or_else(|| std::io::Error::other("config must be set before starting the harness"))?;
    let harness = proxy_world
        .runtime
        .with_ref(|runtime| runtime.block_on(start_harness(config)))
        .ok_or_else(|| std::io::Error::other("runtime must be set"))??;
    proxy_world.harness.set(harness);
    Ok(())
}

#[when("a non-stream chat completions request is sent to the harness")]
fn a_non_stream_chat_completions_request_is_sent_to_the_harness(
    proxy_world: &ProxyWorld,
) -> Result<(), Box<dyn Error>> {
    send_request_to_harness(proxy_world, NON_STREAM_REQUEST, &[])
}

#[when("a non-stream chat completions request with header x-session-secret is sent to the harness")]
fn a_non_stream_chat_completions_request_with_header_is_sent_to_the_harness(
    proxy_world: &ProxyWorld,
) -> Result<(), Box<dyn Error>> {
    send_request_to_harness(
        proxy_world,
        NON_STREAM_REQUEST,
        &[("x-session-secret", "redact-me")],
    )
}

#[when("a non-stream chat completions request with header Authorization is sent to the harness")]
fn a_non_stream_chat_completions_request_with_authorization_is_sent_to_the_harness(
    proxy_world: &ProxyWorld,
) -> Result<(), Box<dyn Error>> {
    send_request_to_harness(
        proxy_world,
        NON_STREAM_REQUEST,
        &[("authorization", "Bearer downstream-secret")],
    )
}

#[then("the client receives the upstream response unchanged")]
fn the_client_receives_the_upstream_response_unchanged(
    proxy_world: &ProxyWorld,
) -> Result<(), Box<dyn Error>> {
    let response = proxy_world
        .response
        .with_ref(Clone::clone)
        .ok_or_else(|| std::io::Error::other("client response should be recorded"))?;
    assert_eq!(response.status, 200);
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&response.body)?,
        sample_success_body(),
    );
    Ok(())
}

#[then("the cassette contains one recorded interaction")]
fn the_cassette_contains_one_recorded_interaction(
    proxy_world: &ProxyWorld,
) -> Result<(), Box<dyn Error>> {
    let cassette = cassette_from_world(proxy_world)?;
    assert_eq!(cassette.interactions.len(), 1);
    Ok(())
}

#[then("the cassette matches the expected snapshot")]
fn the_cassette_matches_expected_snapshot(proxy_world: &ProxyWorld) -> Result<(), Box<dyn Error>> {
    let cassette = cassette_from_world(proxy_world)?;
    assert_cassette_matches_success_snapshot(&cassette)
}

#[then("the upstream receives the request body unchanged")]
fn the_upstream_receives_the_request_body_unchanged(
    proxy_world: &ProxyWorld,
) -> Result<(), Box<dyn Error>> {
    let request = first_upstream_request(proxy_world)?;
    assert_eq!(request.body, NON_STREAM_REQUEST);
    Ok(())
}

#[then("the upstream receives the header x-session-secret")]
fn the_upstream_receives_the_header(proxy_world: &ProxyWorld) -> Result<(), Box<dyn Error>> {
    let request = first_upstream_request(proxy_world)?;
    assert!(
        request
            .headers
            .iter()
            .any(|(name, value)| name == "x-session-secret" && value == "redact-me"),
        "expected forwarded x-session-secret header",
    );
    Ok(())
}

#[then("the upstream does not receive the downstream Authorization header")]
fn the_upstream_does_not_receive_downstream_authorization(
    proxy_world: &ProxyWorld,
) -> Result<(), Box<dyn Error>> {
    let request = first_upstream_request(proxy_world)?;
    assert!(
        request.headers.iter().all(|(name, value)| {
            !name.eq_ignore_ascii_case("authorization") || value != "Bearer downstream-secret"
        }),
        "expected downstream Authorization to be replaced by configured upstream auth",
    );
    assert_upstream_bearer_token(&request)
}

#[then("the upstream receives the configured upstream Bearer token")]
fn the_upstream_receives_configured_upstream_bearer_token(
    proxy_world: &ProxyWorld,
) -> Result<(), Box<dyn Error>> {
    let request = first_upstream_request(proxy_world)?;
    assert_upstream_bearer_token(&request)
}

#[then("the cassette request headers omit x-session-secret")]
fn the_cassette_request_headers_omit_secret(
    proxy_world: &ProxyWorld,
) -> Result<(), Box<dyn Error>> {
    assert_cassette_request_omits_header(proxy_world, "x-session-secret")
}

#[then("the cassette request headers omit Authorization")]
fn the_cassette_request_headers_omit_authorization(
    proxy_world: &ProxyWorld,
) -> Result<(), Box<dyn Error>> {
    assert_cassette_request_omits_header(proxy_world, "authorization")
}

fn assert_cassette_request_omits_header(
    proxy_world: &ProxyWorld,
    header_name: &str,
) -> Result<(), Box<dyn Error>> {
    let cassette = cassette_from_world(proxy_world)?;
    let [interaction] = cassette.interactions.as_slice() else {
        return Err(std::io::Error::other(format!(
            "expected exactly one recorded interaction, got {}",
            cassette.interactions.len()
        ))
        .into());
    };
    if interaction
        .request
        .headers
        .iter()
        .any(|(name, _)| name.eq_ignore_ascii_case(header_name))
    {
        return Err(std::io::Error::other(format!(
            "expected {header_name} to be absent from cassette",
        ))
        .into());
    }
    Ok(())
}

#[then("the harness returns a bad gateway error")]
fn the_harness_returns_a_bad_gateway_error(proxy_world: &ProxyWorld) -> Result<(), Box<dyn Error>> {
    assert_response_status(proxy_world, 502)
}

#[then("the cassette remains empty")]
fn the_cassette_remains_empty(proxy_world: &ProxyWorld) -> Result<(), Box<dyn Error>> {
    let cassette = cassette_from_world(proxy_world)?;
    assert!(cassette.interactions.is_empty());
    Ok(())
}

#[then("the background services shut down cleanly")]
fn the_background_services_shut_down_cleanly(
    proxy_world: &ProxyWorld,
) -> Result<(), Box<dyn Error>> {
    let harness = proxy_world
        .harness
        .take()
        .ok_or_else(|| std::io::Error::other("harness must be running before shutdown"))?;
    let shutdown_result = proxy_world
        .runtime
        .with_ref(|runtime| runtime.block_on(harness.shutdown()))
        .ok_or_else(|| std::io::Error::other("runtime must be set"))?;
    proxy_world.shutdown_result.set(shutdown_result);
    assert!(
        proxy_world
            .shutdown_result
            .with_ref(Result::is_ok)
            .ok_or_else(|| std::io::Error::other("shutdown result should be stored"))?,
        "expected harness shutdown to succeed",
    );

    if let Some(upstream) = proxy_world.upstream.take() {
        upstream.shutdown(
            proxy_world
                .runtime
                .with_ref(Arc::clone)
                .ok_or_else(|| std::io::Error::other("runtime must be set"))?
                .as_ref(),
        )?;
    }

    Ok(())
}

pub(crate) fn send_request_to_harness(
    proxy_world: &ProxyWorld,
    body: &[u8],
    extra_headers: &[(&str, &str)],
) -> Result<(), Box<dyn Error>> {
    let harness_addr = proxy_world
        .harness
        .with_ref(|harness| harness.addr)
        .ok_or_else(|| std::io::Error::other("harness must be running before sending a request"))?;
    let response = proxy_world
        .runtime
        .with_ref(|runtime| send_request(runtime, harness_addr, body, extra_headers))
        .ok_or_else(|| std::io::Error::other("runtime must be set"))??;
    proxy_world.response.set(response);
    Ok(())
}

fn make_record_config(
    cassette_path: &camino::Utf8PathBuf,
    upstream: UpstreamConfig,
    redaction: RedactionConfig,
) -> Result<HarnessConfig, Box<dyn Error>> {
    let cassette_name = cassette_path
        .file_name()
        .ok_or_else(|| std::io::Error::other("cassette path should contain a file name"))?
        .to_owned();
    let cassette_dir = cassette_path
        .parent()
        .ok_or_else(|| std::io::Error::other("cassette path should contain a parent directory"))?
        .to_path_buf();

    Ok(HarnessConfig {
        listen: ListenAddr::from(SocketAddr::from(([127, 0, 0, 1], 0))),
        mode: Mode::Record,
        cassette_dir,
        cassette_name,
        upstream: Some(upstream),
        redaction,
        ..HarnessConfig::default()
    })
}

fn configure_harness_with_proxy_world_upstream(
    proxy_world: &ProxyWorld,
    cassette_label: &str,
    redaction: RedactionConfig,
) -> Result<(), Box<dyn Error>> {
    let upstream = proxy_world
        .upstream
        .with_ref(StubUpstream::base_url)
        .ok_or_else(|| std::io::Error::other("stub upstream must be configured"))?;
    let cassette_path = unique_cassette_path(cassette_label);
    proxy_world.cassette_path.set(cassette_path.clone());
    proxy_world.config.set(make_record_config(
        &cassette_path,
        UpstreamConfig {
            kind: UpstreamKind::OpenRouter,
            base_url: upstream,
            api_key_env: present_env_name()?.to_owned(),
            extra_headers: std::collections::BTreeMap::new(),
        },
        redaction,
    )?);
    Ok(())
}

pub(crate) fn cassette_from_world(
    proxy_world: &ProxyWorld,
) -> Result<spycatcher_harness::cassette::Cassette, Box<dyn Error>> {
    let cassette_path = proxy_world
        .cassette_path
        .with_ref(Clone::clone)
        .ok_or_else(|| std::io::Error::other("cassette path should be recorded"))?;
    load_cassette(&cassette_path)
}

pub(crate) fn first_upstream_request(
    proxy_world: &ProxyWorld,
) -> Result<CapturedRequest, Box<dyn Error>> {
    let requests = proxy_world
        .upstream
        .with_ref(StubUpstream::captured_requests)
        .ok_or_else(|| std::io::Error::other("stub upstream must be available"))??;
    let [request] = requests.as_slice() else {
        return Err(std::io::Error::other(format!(
            "expected exactly one proxied request, got {}",
            requests.len()
        ))
        .into());
    };
    Ok(request.clone())
}

#[expect(
    clippy::panic_in_result_fn,
    reason = "BDD assertion helper preserves step assertion behaviour"
)]
fn assert_response_status(proxy_world: &ProxyWorld, expected: u16) -> Result<(), Box<dyn Error>> {
    let response = proxy_world
        .response
        .with_ref(Clone::clone)
        .ok_or_else(|| std::io::Error::other("client response should be recorded"))?;
    assert_eq!(response.status, expected);
    Ok(())
}
