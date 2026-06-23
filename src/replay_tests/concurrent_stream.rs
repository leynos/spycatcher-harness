//! Concurrent stream replay tests.

use rstest::rstest;

use super::*;

#[rstest]
fn concurrent_sequential_replay_preserves_stream_events_once_each() {
    let request = sample_observed_request(br#"{"model":"test","stream":true,"messages":[]}"#);
    let cassette = cassette_with_duplicate_stream_requests(request.clone(), 8)
        .expect("requests should canonicalize");
    let service = ReplayService::new(
        ReplayMatchEngine::new(cassette, MatchMode::SequentialStrict)
            .expect("cassette should build replay engine"),
    );
    let handles = (0..8)
        .map(|_| {
            let replay_service = service.clone();
            let replay_request = request.clone();
            std::thread::spawn(move || {
                replay_service
                    .handle_chat_completions(replay_request)
                    .expect("duplicate stream request should replay")
                    .body
            })
        })
        .collect::<Vec<_>>();

    let mut event_ids = handles
        .into_iter()
        .map(|handle| handle.join().expect("thread should not panic"))
        .map(|body| match body {
            ReplayBody::Events(events) => assert_stream_event_order(&events),
            ReplayBody::OneShot(_) => panic!("duplicate stream responses should be events"),
        })
        .collect::<Vec<_>>();
    event_ids.sort();

    assert_eq!(
        event_ids,
        (0..8)
            .map(|index| format!("stream-response-{index}"))
            .collect::<Vec<_>>()
    );
    assert_eq!(service.counters(), (8, 0));
}
