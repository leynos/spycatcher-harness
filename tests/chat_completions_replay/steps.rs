//! Step definitions for chat completions replay scenarios.

use std::error::Error;
use std::net::SocketAddr;
use std::sync::Arc;

use rstest_bdd_macros::{given, then, when};
use url::Url;

use spycatcher_harness::config::{ListenAddr, Mode, UpstreamConfig, UpstreamKind};
use spycatcher_harness::{HarnessConfig, RunningHarness, start_harness};

use crate::chat_completions_replay::support::{
    assert_response_error_kind, replay_response, response_error, response_error_kind,
};
use crate::chat_completions_replay::world::ReplayWorld;
use crate::record_helpers::{
    ClientResponse, StubUpstream, load_cassette, present_env_name, sample_success_body,
    send_request, unique_cassette_path,
};

const BASELINE_REQUEST: &[u8] =
    br#"{"model":"gpt-test","messages":[{"role":"user","content":"hi"}]}"#;
const DIFFERENT_REQUEST: &[u8] =
    br#"{"model":"gpt-other","messages":[{"role":"user","content":"hi"}]}"#;
const STREAMING_REQUEST: &[u8] =
    br#"{"model":"gpt-test","stream":true,"messages":[{"role":"user","content":"hi"}]}"#;
const MALFORMED_REQUEST: &[u8] = br#"{"model":"gpt-test""#;
const DIFFERENT_MALFORMED_REQUEST: &[u8] = br#"{"model":"gpt-other""#;

struct HarnessTarget<'a> {
    harness_slot: &'a rstest_bdd::Slot<RunningHarness>,
    response_slot: &'a rstest_bdd::Slot<ClientResponse>,
    label: &'a str,
}

#[given("a stub upstream that returns a successful chat completion for replay")]
fn a_stub_upstream_that_returns_a_successful_chat_completion_for_replay(
    replay_world: &ReplayWorld,
) -> Result<(), Box<dyn Error>> {
    let upstream = replay_world
        .runtime
        .with_ref(|runtime| StubUpstream::start(runtime, sample_success_body()))
        .ok_or_else(|| std::io::Error::other("runtime must be set"))??;
    replay_world.upstream.set(upstream);
    Ok(())
}

#[given("a record-mode harness configured for replay setup")]
fn a_record_mode_harness_configured_for_replay_setup(
    replay_world: &ReplayWorld,
) -> Result<(), Box<dyn Error>> {
    let upstream = replay_world
        .upstream
        .with_ref(StubUpstream::base_url)
        .ok_or_else(|| std::io::Error::other("stub upstream must be configured"))?;
    let cassette_path = unique_cassette_path("replay");
    replay_world.cassette_path.set(cassette_path.clone());
    replay_world.record_config.set(make_record_config(
        &cassette_path,
        UpstreamConfig {
            kind: UpstreamKind::OpenRouter,
            base_url: upstream,
            api_key_env: present_env_name()?.to_owned(),
            extra_headers: std::collections::BTreeMap::new(),
        },
    )?);
    Ok(())
}

#[when("the record harness is started")]
fn the_record_harness_is_started(replay_world: &ReplayWorld) -> Result<(), Box<dyn Error>> {
    start_harness_from_slot(
        replay_world,
        &replay_world.record_config,
        &replay_world.record_harness,
        "record config",
    )
}

#[when("the baseline non-stream request is sent to the record harness")]
fn the_baseline_non_stream_request_is_sent_to_the_record_harness(
    replay_world: &ReplayWorld,
) -> Result<(), Box<dyn Error>> {
    send_request_to_record_harness(replay_world, BASELINE_REQUEST)
}

#[when("a malformed JSON request is sent to the record harness")]
fn a_malformed_json_request_is_sent_to_the_record_harness(
    replay_world: &ReplayWorld,
) -> Result<(), Box<dyn Error>> {
    send_request_to_record_harness(replay_world, MALFORMED_REQUEST)
}

#[when("the record harness is stopped")]
fn the_record_harness_is_stopped(replay_world: &ReplayWorld) -> Result<(), Box<dyn Error>> {
    stop_harness_from_slot(replay_world, &replay_world.record_harness, "record harness")
}

#[when("a replay-mode harness is configured from the recorded cassette")]
fn a_replay_mode_harness_is_configured_from_the_recorded_cassette(
    replay_world: &ReplayWorld,
) -> Result<(), Box<dyn Error>> {
    let cassette_path = replay_world
        .cassette_path
        .with_ref(Clone::clone)
        .ok_or_else(|| std::io::Error::other("cassette path must be set"))?;
    let cassette = load_cassette(&cassette_path)?;
    assert_eq!(cassette.interactions.len(), 1);
    replay_world
        .replay_config
        .set(make_replay_config(&cassette_path)?);
    Ok(())
}

#[when("the replay harness is started")]
fn the_replay_harness_is_started(replay_world: &ReplayWorld) -> Result<(), Box<dyn Error>> {
    start_harness_from_slot(
        replay_world,
        &replay_world.replay_config,
        &replay_world.replay_harness,
        "replay config",
    )
}

#[when("the baseline non-stream request is sent to the replay harness")]
fn the_baseline_non_stream_request_is_sent_to_the_replay_harness(
    replay_world: &ReplayWorld,
) -> Result<(), Box<dyn Error>> {
    send_replay_request(replay_world, BASELINE_REQUEST)
}

#[when("a different non-stream request is sent to the replay harness")]
fn a_different_non_stream_request_is_sent_to_the_replay_harness(
    replay_world: &ReplayWorld,
) -> Result<(), Box<dyn Error>> {
    send_replay_request(replay_world, DIFFERENT_REQUEST)
}

#[when("a streaming request is sent to the replay harness")]
fn a_streaming_request_is_sent_to_the_replay_harness(
    replay_world: &ReplayWorld,
) -> Result<(), Box<dyn Error>> {
    send_replay_request(replay_world, STREAMING_REQUEST)
}

#[when("a different malformed JSON request is sent to the replay harness")]
fn a_different_malformed_json_request_is_sent_to_the_replay_harness(
    replay_world: &ReplayWorld,
) -> Result<(), Box<dyn Error>> {
    send_replay_request(replay_world, DIFFERENT_MALFORMED_REQUEST)
}

#[then("the replay client receives the recorded response unchanged")]
fn the_replay_client_receives_the_recorded_response_unchanged(
    replay_world: &ReplayWorld,
) -> Result<(), Box<dyn Error>> {
    let record = replay_world
        .record_response
        .with_ref(Clone::clone)
        .ok_or_else(|| std::io::Error::other("record response must be stored"))?;
    let replay = replay_response(replay_world)?;
    assert_eq!(replay.status, record.status);
    assert_eq!(replay.body, record.body);
    assert!(
        replay
            .headers
            .iter()
            .any(|(name, value)| name == "content-type" && value.starts_with("application/json")),
        "expected replay response to include recorded content-type",
    );
    Ok(())
}

#[then("the stub upstream saw no replay request")]
fn the_stub_upstream_saw_no_replay_request(
    replay_world: &ReplayWorld,
) -> Result<(), Box<dyn Error>> {
    let requests = replay_world
        .upstream
        .with_ref(StubUpstream::captured_requests)
        .ok_or_else(|| std::io::Error::other("stub upstream must be available"))??;
    assert_eq!(requests.len(), 1);
    Ok(())
}

#[then("the replay client receives a request mismatch diagnostic")]
fn the_replay_client_receives_a_request_mismatch_diagnostic(
    replay_world: &ReplayWorld,
) -> Result<(), Box<dyn Error>> {
    let response = replay_response(replay_world)?;
    assert_eq!(response.status, 409);
    let body: serde_json::Value = serde_json::from_slice(&response.body)?;
    let error = response_error(&body)?;
    assert_eq!(response_error_kind(error), Some("request_mismatch"));
    assert!(
        error
            .get("expected_hash")
            .and_then(serde_json::Value::as_str)
            .is_some()
    );
    assert!(
        error
            .get("observed_hash")
            .and_then(serde_json::Value::as_str)
            .is_some()
    );
    assert!(
        error
            .get("diff_summary")
            .and_then(serde_json::Value::as_str)
            .is_some()
    );
    Ok(())
}

#[then("the replay client receives an unsupported streaming response")]
fn the_replay_client_receives_an_unsupported_streaming_response(
    replay_world: &ReplayWorld,
) -> Result<(), Box<dyn Error>> {
    let response = replay_response(replay_world)?;
    assert_eq!(response.status, 501);
    let body: serde_json::Value = serde_json::from_slice(&response.body)?;
    assert_response_error_kind(&body, "unsupported_stream")?;
    Ok(())
}

#[then("the replay client receives a malformed JSON response")]
fn the_replay_client_receives_a_malformed_json_response(
    replay_world: &ReplayWorld,
) -> Result<(), Box<dyn Error>> {
    let response = replay_response(replay_world)?;
    assert_eq!(response.status, 400);
    let body: serde_json::Value = serde_json::from_slice(&response.body)?;
    assert_response_error_kind(&body, "malformed_json")?;
    Ok(())
}

#[then("the replay harness is stopped")]
fn the_replay_harness_is_stopped(replay_world: &ReplayWorld) -> Result<(), Box<dyn Error>> {
    stop_harness_from_slot(replay_world, &replay_world.replay_harness, "replay harness")
}

#[then("the replay stub upstream is stopped")]
fn the_replay_stub_upstream_is_stopped(replay_world: &ReplayWorld) -> Result<(), Box<dyn Error>> {
    let Some(upstream) = replay_world.upstream.take() else {
        return Ok(());
    };
    upstream.shutdown(
        replay_world
            .runtime
            .with_ref(Arc::clone)
            .ok_or_else(|| std::io::Error::other("runtime must be set"))?
            .as_ref(),
    )
}

fn send_replay_request(replay_world: &ReplayWorld, body: &[u8]) -> Result<(), Box<dyn Error>> {
    send_request_to_harness_slot(
        replay_world,
        &HarnessTarget {
            harness_slot: &replay_world.replay_harness,
            response_slot: &replay_world.replay_response,
            label: "replay harness",
        },
        body,
    )
}

fn send_request_to_record_harness(
    replay_world: &ReplayWorld,
    body: &[u8],
) -> Result<(), Box<dyn Error>> {
    send_request_to_harness_slot(
        replay_world,
        &HarnessTarget {
            harness_slot: &replay_world.record_harness,
            response_slot: &replay_world.record_response,
            label: "record harness",
        },
        body,
    )
}

fn start_harness_from_slot(
    world: &ReplayWorld,
    config_slot: &rstest_bdd::Slot<HarnessConfig>,
    harness_slot: &rstest_bdd::Slot<RunningHarness>,
    config_label: &str,
) -> Result<(), Box<dyn Error>> {
    let config = config_slot
        .take()
        .ok_or_else(|| std::io::Error::other(format!("{config_label} must be set")))?;
    let harness = world
        .runtime
        .with_ref(|runtime| runtime.block_on(start_harness(config)))
        .ok_or_else(|| std::io::Error::other("runtime must be set"))??;
    harness_slot.set(harness);
    Ok(())
}

fn stop_harness_from_slot(
    world: &ReplayWorld,
    harness_slot: &rstest_bdd::Slot<RunningHarness>,
    harness_label: &str,
) -> Result<(), Box<dyn Error>> {
    let harness = harness_slot
        .take()
        .ok_or_else(|| std::io::Error::other(format!("{harness_label} must be running")))?;
    world
        .runtime
        .with_ref(|runtime| runtime.block_on(harness.shutdown()))
        .ok_or_else(|| std::io::Error::other("runtime must be set"))??;
    Ok(())
}

fn send_request_to_harness_slot(
    world: &ReplayWorld,
    target: &HarnessTarget<'_>,
    body: &[u8],
) -> Result<(), Box<dyn Error>> {
    let harness_addr = target
        .harness_slot
        .with_ref(|harness| harness.addr)
        .ok_or_else(|| std::io::Error::other(format!("{} must be running", target.label)))?;
    let response = world
        .runtime
        .with_ref(|runtime| send_request(runtime, harness_addr, body, &[]))
        .ok_or_else(|| std::io::Error::other("runtime must be set"))??;
    target.response_slot.set(response);
    Ok(())
}

fn make_record_config(
    cassette_path: &camino::Utf8PathBuf,
    upstream: UpstreamConfig,
) -> Result<HarnessConfig, Box<dyn Error>> {
    let (cassette_dir, cassette_name) = split_cassette_path(cassette_path)?;

    Ok(HarnessConfig {
        listen: ListenAddr::from(SocketAddr::from(([127, 0, 0, 1], 0))),
        mode: Mode::Record,
        cassette_dir,
        cassette_name,
        upstream: Some(upstream),
        ..HarnessConfig::default()
    })
}

fn make_replay_config(
    cassette_path: &camino::Utf8PathBuf,
) -> Result<HarnessConfig, Box<dyn Error>> {
    let (cassette_dir, cassette_name) = split_cassette_path(cassette_path)?;
    let base_url = Url::parse("http://127.0.0.1:1/api/v1")
        .map_err(|error| std::io::Error::other(format!("test fixture URL is invalid: {error}")))?;

    Ok(HarnessConfig {
        listen: ListenAddr::from(SocketAddr::from(([127, 0, 0, 1], 0))),
        mode: Mode::Replay,
        cassette_dir,
        cassette_name,
        upstream: Some(UpstreamConfig {
            kind: UpstreamKind::OpenRouter,
            base_url,
            api_key_env: "SPYCATCHER_REPLAY_SHOULD_NOT_READ".to_owned(),
            extra_headers: std::collections::BTreeMap::new(),
        }),
        ..HarnessConfig::default()
    })
}

fn split_cassette_path(
    cassette_path: &camino::Utf8PathBuf,
) -> Result<(camino::Utf8PathBuf, String), Box<dyn Error>> {
    let cassette_name = cassette_path
        .file_name()
        .ok_or_else(|| std::io::Error::other("cassette path should contain a file name"))?
        .to_owned();
    let cassette_dir = cassette_path
        .parent()
        .ok_or_else(|| std::io::Error::other("cassette path should contain a parent directory"))?
        .to_path_buf();
    Ok((cassette_dir, cassette_name))
}
