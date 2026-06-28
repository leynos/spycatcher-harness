//! Stream-specific replay BDD step definitions.

use std::error::Error;

use camino::Utf8Path;
use cap_std::{ambient_authority, fs_utf8::Dir};
use rstest_bdd_macros::{given, then, when};
use spycatcher_harness::cassette::{
    Cassette, IgnorePathConfig, Interaction, InteractionMetadata, RecordedRequest,
    RecordedResponse, StreamCanonicalPolicy, StreamEvent, canonicalize_events,
};

use crate::chat_completions_replay::steps::{
    STREAMING_REQUEST, make_replay_config, send_replay_request, send_request_to_record_harness,
};
use crate::chat_completions_replay::support::{assert_response_error_kind, replay_response};
use crate::chat_completions_replay::world::ReplayWorld;
use crate::record_helpers::{StubUpstream, load_cassette, unique_cassette_path};

#[given("a stub upstream that returns an OpenRouter stream for replay")]
fn a_stub_upstream_that_returns_an_open_router_stream_for_replay(
    replay_world: &ReplayWorld,
) -> Result<(), Box<dyn Error>> {
    let upstream = replay_world
        .runtime
        .with_ref(|runtime| StubUpstream::start_stream(runtime, sample_stream_transcript()))
        .ok_or_else(|| std::io::Error::other("runtime must be set"))??;
    replay_world.upstream.set(upstream);
    Ok(())
}

#[given("a replay-mode harness is configured from a stream-shaped non-stream cassette")]
fn a_replay_mode_harness_is_configured_from_a_stream_shaped_non_stream_cassette(
    replay_world: &ReplayWorld,
) -> Result<(), Box<dyn Error>> {
    let cassette_path = unique_cassette_path("stream-missing");
    let cassette = stream_shaped_non_stream_cassette()?;
    write_cassette(&cassette_path, &cassette)?;
    replay_world.cassette_path.set(cassette_path.clone());
    replay_world
        .replay_config
        .set(make_replay_config(&cassette_path)?);
    Ok(())
}

#[when("a streaming request is sent to the record harness")]
fn a_streaming_request_is_sent_to_the_record_harness(
    replay_world: &ReplayWorld,
) -> Result<(), Box<dyn Error>> {
    send_request_to_record_harness(replay_world, STREAMING_REQUEST)
}

#[when("a replay-mode harness is configured from the recorded stream cassette")]
fn a_replay_mode_harness_is_configured_from_the_recorded_stream_cassette(
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

#[when("the matching streaming request is sent to the replay harness")]
fn the_matching_streaming_request_is_sent_to_the_replay_harness(
    replay_world: &ReplayWorld,
) -> Result<(), Box<dyn Error>> {
    send_replay_request(replay_world, STREAMING_REQUEST)
}

#[then("the replay client receives the recorded stream with comment frames")]
fn the_replay_client_receives_the_recorded_stream_with_comment_frames(
    replay_world: &ReplayWorld,
) -> Result<(), Box<dyn Error>> {
    let response = replay_response(replay_world)?;
    assert_eq!(response.status, 200);
    assert_eq!(response.body, sample_stream_transcript());
    assert!(response.headers.iter().any(|(name, value)| {
        name.eq_ignore_ascii_case("content-type")
            && value.split(';').next().is_some_and(|media_type| {
                media_type.trim().eq_ignore_ascii_case("text/event-stream")
            })
    }));
    Ok(())
}

#[then("the replay client receives a stream cassette required response")]
fn the_replay_client_receives_a_stream_cassette_required_response(
    replay_world: &ReplayWorld,
) -> Result<(), Box<dyn Error>> {
    let response = replay_response(replay_world)?;
    assert_eq!(response.status, 501);
    let body: serde_json::Value = serde_json::from_slice(&response.body)?;
    assert_response_error_kind(&body, "stream_cassette_required")?;
    Ok(())
}

#[given("canonical stream events with different comment text")]
fn canonical_stream_events_with_different_comment_text(replay_world: &ReplayWorld) {
    replay_world.canonical_expected.set(vec![
        comment("OPENROUTER PROCESSING"),
        data("{\"id\":\"chunk\"}"),
        data("[DONE]"),
    ]);
    replay_world.canonical_observed.set(vec![
        comment("OPENROUTER STILL PROCESSING"),
        data("{\"id\":\"chunk\"}"),
        data("[DONE]"),
    ]);
}

#[then("canonical-stream comparison treats the streams as equivalent")]
fn canonical_stream_comparison_treats_the_streams_as_equivalent(
    replay_world: &ReplayWorld,
) -> Result<(), Box<dyn Error>> {
    let expected = replay_world
        .canonical_expected
        .with_ref(Clone::clone)
        .ok_or_else(|| std::io::Error::other("expected stream events must be set"))?;
    let observed = replay_world
        .canonical_observed
        .with_ref(Clone::clone)
        .ok_or_else(|| std::io::Error::other("observed stream events must be set"))?;
    let policy = StreamCanonicalPolicy::ignore_comments();

    assert_ne!(expected, observed);
    assert_eq!(
        canonicalize_events(&expected, policy),
        canonicalize_events(&observed, policy),
    );
    Ok(())
}

fn sample_stream_transcript() -> Vec<u8> {
    concat!(
        ": OPENROUTER PROCESSING\n\n",
        "data: {\"id\":\"chatcmpl-test\",\"object\":\"chat.completion.chunk\",",
        "\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\"},",
        "\"finish_reason\":null}],\"usage\":null}\n\n",
        "data: {\"id\":\"chatcmpl-test\",\"object\":\"chat.completion.chunk\",",
        "\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Hello\"},",
        "\"finish_reason\":null}],\"usage\":null}\n\n",
        "data: {\"id\":\"chatcmpl-test\",\"object\":\"chat.completion.chunk\",",
        "\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}],",
        "\"usage\":null}\n\n",
        "data: {\"id\":\"chatcmpl-test\",\"object\":\"chat.completion.chunk\",",
        "\"choices\":[],\"usage\":{\"prompt_tokens\":1,\"completion_tokens\":1,",
        "\"total_tokens\":2}}\n\n",
        "data: [DONE]\n\n",
    )
    .as_bytes()
    .to_vec()
}

fn comment(text: &str) -> StreamEvent {
    StreamEvent::Comment {
        text: text.to_owned(),
    }
}

fn data(raw: &str) -> StreamEvent {
    StreamEvent::Data {
        raw: raw.to_owned(),
        parsed_json: None,
    }
}

fn stream_shaped_non_stream_cassette() -> Result<Cassette, Box<dyn Error>> {
    let parsed_json = serde_json::from_slice(STREAMING_REQUEST)?;
    let mut request = RecordedRequest {
        method: "POST".to_owned(),
        path: "/v1/chat/completions".to_owned(),
        query: String::new(),
        headers: vec![("content-type".to_owned(), "application/json".to_owned())],
        body: STREAMING_REQUEST.to_vec(),
        parsed_json: Some(parsed_json),
        canonical_request: None,
        stable_hash: None,
    };
    request.populate_canonical_fields(&IgnorePathConfig::default())?;
    let mut cassette = Cassette::new();
    cassette.interactions.push(Interaction {
        request,
        response: RecordedResponse::NonStream {
            status: 200,
            headers: Vec::new(),
            body: b"not a stream".to_vec(),
            parsed_json: None,
        },
        metadata: InteractionMetadata {
            protocol_id: "openai.chat_completions.v1".to_owned(),
            upstream_id: "test".to_owned(),
            recorded_at: "2026-05-08T00:00:00Z".to_owned(),
            relative_offset_ms: 0,
        },
    });
    Ok(cassette)
}

fn write_cassette(path: &Utf8Path, cassette: &Cassette) -> Result<(), Box<dyn Error>> {
    let root = Dir::open_ambient_dir(".", ambient_authority())?;
    let parent = path
        .parent()
        .ok_or_else(|| std::io::Error::other(format!("cassette path needs parent: {path}")))?;
    root.create_dir_all(parent)?;
    let parent_dir = root.open_dir(parent)?;
    let file_name = path
        .file_name()
        .ok_or_else(|| std::io::Error::other(format!("cassette path needs file name: {path}")))?;
    let mut file = parent_dir.create(file_name)?;
    cassette.write_to(&mut file)?;
    file.sync_all()?;
    Ok(())
}
