//! Stream-specific record-mode BDD step definitions.

use std::error::Error;

use rstest_bdd_macros::{given, then, when};
use spycatcher_harness::cassette::{RecordedResponse, StreamEvent};

use crate::record_mode_proxying::helpers::StubUpstream;
use crate::record_mode_proxying::steps::{
    STREAMING_REQUEST, cassette_from_world, first_upstream_request, send_request_to_harness,
};
use crate::record_mode_proxying::world::ProxyWorld;

#[given("a stub upstream that returns an OpenAI-style SSE stream")]
fn a_stub_upstream_that_returns_an_openai_style_sse_stream(
    proxy_world: &ProxyWorld,
) -> Result<(), Box<dyn Error>> {
    let upstream = proxy_world
        .runtime
        .with_ref(|runtime| StubUpstream::start_stream(runtime, sample_stream_transcript()))
        .transpose()?
        .ok_or_else(|| std::io::Error::other("runtime must be set"))?;
    proxy_world.upstream.set(upstream);
    Ok(())
}

#[when("a streaming chat completions request is sent to the harness")]
fn a_streaming_chat_completions_request_is_sent_to_the_harness(
    proxy_world: &ProxyWorld,
) -> Result<(), Box<dyn Error>> {
    send_request_to_harness(proxy_world, STREAMING_REQUEST, &[])
}

#[then("the client receives the upstream stream transcript unchanged")]
fn the_client_receives_the_upstream_stream_transcript_unchanged(
    proxy_world: &ProxyWorld,
) -> Result<(), Box<dyn Error>> {
    let response = proxy_world
        .response
        .with_ref(Clone::clone)
        .ok_or_else(|| std::io::Error::other("client response should be recorded"))?;
    assert_eq!(response.status, 200);
    assert_eq!(response.body, sample_stream_transcript());
    assert!(has_event_stream_content_type(&response.headers));
    Ok(())
}

#[then("the upstream receives the streaming request body unchanged")]
fn the_upstream_receives_the_streaming_request_body_unchanged(
    proxy_world: &ProxyWorld,
) -> Result<(), Box<dyn Error>> {
    let request = first_upstream_request(proxy_world)?;
    assert_eq!(request.body, STREAMING_REQUEST);
    Ok(())
}

#[then("the cassette contains one recorded stream interaction")]
fn the_cassette_contains_one_recorded_stream_interaction(
    proxy_world: &ProxyWorld,
) -> Result<(), Box<dyn Error>> {
    let cassette = cassette_from_world(proxy_world)?;
    let [interaction] = cassette.interactions.as_slice() else {
        return Err(std::io::Error::other(format!(
            "expected exactly one recorded interaction, got {}",
            cassette.interactions.len()
        ))
        .into());
    };
    let RecordedResponse::Stream {
        status,
        headers,
        events,
        raw_transcript,
        timing,
    } = &interaction.response
    else {
        return Err(std::io::Error::other("expected stream response in cassette").into());
    };
    assert_eq!(*status, 200);
    assert_eq!(raw_transcript, &sample_stream_transcript());
    assert_stream_headers(headers);
    assert_stream_events(events, timing.is_some());
    Ok(())
}

fn assert_stream_headers(headers: &[(String, String)]) {
    assert!(has_event_stream_content_type(headers));
}

fn has_event_stream_content_type(headers: &[(String, String)]) -> bool {
    headers.iter().any(|(name, value)| {
        name.eq_ignore_ascii_case("content-type") && value.starts_with("text/event-stream")
    })
}

fn assert_stream_events(events: &[StreamEvent], has_timing: bool) {
    assert!(has_timing, "expected stream timing metadata");
    assert!(matches!(
        events.first(),
        Some(StreamEvent::Comment { text }) if text == "OPENROUTER PROCESSING"
    ));
    assert!(matches!(
        events.last(),
        Some(StreamEvent::Data { raw, parsed_json: None }) if raw == "[DONE]"
    ));
    assert!(
        events.iter().any(|event| matches!(
            event,
            StreamEvent::Data {
                parsed_json: Some(value),
                ..
            } if value.get("usage").is_some_and(|usage| !usage.is_null())
        )),
        "expected usage final chunk to be parsed",
    );
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
