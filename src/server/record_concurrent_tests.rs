//! Concurrent record-mode stress test.
use super::record_tests_helpers::*;

use crate::http_exchange::{ObservedRequest, parse_json_bytes};
use crate::protocol::CHAT_COMPLETIONS_PATH;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_requests_are_recorded_without_data_loss() {
    let cassette = cassette_fixture("concurrent");
    let body = br#"{"model":"gpt-test","messages":[]}"#;

    let service = std::sync::Arc::new(service_fixture(
        &cassette.path,
        FakeUpstream {
            response: Ok(sample_response(br#"{"id":"ok"}"#)),
        },
        FakeEnvProvider(Some("concurrent-key".to_owned())),
    ));

    let handles: Vec<_> = (0..8)
        .map(|_| {
            let svc = std::sync::Arc::clone(&service);
            let req = ObservedRequest {
                method: "POST".to_owned(),
                path: CHAT_COMPLETIONS_PATH.to_owned(),
                query: String::new(),
                headers: vec![("content-type".to_owned(), "application/json".to_owned())],
                forward_headers: vec![("content-type".to_owned(), b"application/json".to_vec())],
                body: body.to_vec(),
                parsed_json: parse_json_bytes(body),
            };
            tokio::spawn(async move {
                svc.handle_chat_completions(req)
                    .await
                    .expect("concurrent request should succeed")
            })
        })
        .collect();

    for handle in handles {
        handle.await.expect("task should not panic");
    }

    let persisted = load_cassette(&cassette.path);
    assert_eq!(
        (persisted.interactions.len(), service.counters()),
        (8, (8, 0)),
        "all eight concurrent interactions must be persisted and counted"
    );
}
