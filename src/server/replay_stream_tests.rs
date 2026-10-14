//! Unit tests for replay stream serialization.

use std::sync::{Arc, Mutex};

use axum::body::to_bytes;
use metrics::{
    Counter, CounterFn, Gauge, Histogram, Key, KeyName, Metadata, Recorder, SharedString, Unit,
};
use proptest::prelude::*;
use rstest::rstest;

use super::*;
use crate::protocol::{CHAT_COMPLETIONS_PATH, CHAT_COMPLETIONS_PROTOCOL_ID};

#[rstest]
#[tokio::test]
async fn parsed_event_replay_emits_data_frames() {
    let body = build_stream_body(vec![data("{\"id\":\"chunk\"}"), data("[DONE]")], &labels());

    let bytes = to_bytes(body, 1024).await.expect("body should be readable");

    assert_eq!(
        bytes.as_ref(),
        b"data: {\"id\":\"chunk\"}\n\ndata: [DONE]\n\n"
    );
}

#[rstest]
#[tokio::test]
async fn parsed_event_replay_emits_comment_frames_in_order() {
    let body = build_stream_body(
        vec![
            data("{\"id\":\"first\"}"),
            comment("OPENROUTER PROCESSING"),
            data("[DONE]"),
        ],
        &labels(),
    );

    let bytes = to_bytes(body, 1024).await.expect("body should be readable");

    assert_eq!(
        bytes.as_ref(),
        b"data: {\"id\":\"first\"}\n\n: OPENROUTER PROCESSING\n\ndata: [DONE]\n\n",
    );
}

#[rstest]
#[tokio::test]
async fn empty_event_list_produces_empty_body() {
    let body = build_stream_body(Vec::new(), &labels());

    let bytes = to_bytes(body, 1024).await.expect("body should be readable");

    assert!(bytes.is_empty());
}

#[rstest]
#[tokio::test]
async fn parsed_event_replay_splits_multiline_payloads() {
    let body = build_stream_body(vec![data("alpha\nbeta"), comment("one\ntwo")], &labels());

    let bytes = to_bytes(body, 1024).await.expect("body should be readable");

    assert_eq!(
        bytes.as_ref(),
        b"data: alpha\ndata: beta\n\n: one\n: two\n\n"
    );
}

#[rstest]
#[tokio::test]
async fn eager_stream_limit_includes_boundary_size() {
    assert_stream_body_len_for_payload(
        EAGER_STREAM_LIMIT_BYTES - 8,
        EAGER_STREAM_LIMIT_BYTES + 1,
        EAGER_STREAM_LIMIT_BYTES,
    )
    .await
    .expect("body should be readable");
}

#[rstest]
#[tokio::test]
async fn oversized_stream_body_remains_readable() {
    assert_stream_body_len_for_payload(
        EAGER_STREAM_LIMIT_BYTES - 7,
        EAGER_STREAM_LIMIT_BYTES + 2,
        EAGER_STREAM_LIMIT_BYTES + 1,
    )
    .await
    .expect("body should be readable");
}

#[rstest]
#[tokio::test]
async fn oversized_stream_body_records_streamed_delivery_metric() {
    let recorder = StreamMetricRecorder::default();
    let _recorder_guard = metrics::set_default_local_recorder(&recorder);

    let body = build_stream_body(
        vec![data(&"x".repeat(EAGER_STREAM_LIMIT_BYTES - 7))],
        &labels(),
    );
    let _bytes = to_bytes(body, EAGER_STREAM_LIMIT_BYTES + 2)
        .await
        .expect("body should be readable");

    assert!(
        recorder.has_counter_with_label(
            "spycatcher.replay.stream.delivery.total",
            "delivery",
            StreamDeliveryMode::Streamed.as_str(),
        ),
        "over-limit stream replay should record streamed delivery",
    );
}

proptest! {
    #[test]
    fn serialized_stream_event_lines_keep_sse_prefix(
        event in stream_event_with_newlines(),
    ) {
        let expected_prefix = match &event {
            StreamEvent::Comment { .. } => ":",
            StreamEvent::Data { .. } => "data:",
        };
        let serialized = String::from_utf8(serialize_event(&event))
            .expect("serialized SSE should be UTF-8");

        for line in serialized.lines().filter(|line| !line.is_empty()) {
            prop_assert!(
                line.starts_with(expected_prefix),
                "line {line:?} should start with {expected_prefix:?}",
            );
        }
    }
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

async fn assert_stream_body_len_for_payload(
    payload_len: usize,
    read_limit: usize,
    expected_len: usize,
) -> Result<(), axum::Error> {
    let body = build_stream_body(vec![data(&"x".repeat(payload_len))], &labels());

    let bytes = to_bytes(body, read_limit).await?;

    assert_eq!(bytes.len(), expected_len);
    Ok(())
}

fn labels() -> ReplayMetricLabels {
    ReplayMetricLabels::new(CHAT_COMPLETIONS_PROTOCOL_ID, CHAT_COMPLETIONS_PATH)
}

#[derive(Default)]
struct StreamMetricRecorder {
    counters: Arc<Mutex<Vec<Key>>>,
}

impl StreamMetricRecorder {
    fn has_counter_with_label(
        &self,
        metric_name: &str,
        label_key: &str,
        label_value: &str,
    ) -> bool {
        let counters = match self.counters.lock() {
            Ok(counters) => counters,
            Err(poisoned) => poisoned.into_inner(),
        };
        counters.iter().any(|key| {
            key.name() == metric_name
                && key
                    .labels()
                    .any(|label| label.key() == label_key && label.value() == label_value)
        })
    }
}

impl Recorder for StreamMetricRecorder {
    fn describe_counter(&self, _key: KeyName, _unit: Option<Unit>, _description: SharedString) {}

    fn describe_gauge(&self, _key: KeyName, _unit: Option<Unit>, _description: SharedString) {}

    fn describe_histogram(&self, _key: KeyName, _unit: Option<Unit>, _description: SharedString) {}

    fn register_counter(&self, key: &Key, _metadata: &Metadata<'_>) -> Counter {
        Counter::from_arc(Arc::new(StreamMetricCounter {
            key: key.clone(),
            counters: Arc::clone(&self.counters),
        }))
    }

    fn register_gauge(&self, _key: &Key, _metadata: &Metadata<'_>) -> Gauge {
        Gauge::noop()
    }

    fn register_histogram(&self, _key: &Key, _metadata: &Metadata<'_>) -> Histogram {
        Histogram::noop()
    }
}

struct StreamMetricCounter {
    key: Key,
    counters: Arc<Mutex<Vec<Key>>>,
}

impl CounterFn for StreamMetricCounter {
    fn increment(&self, _value: u64) {
        match self.counters.lock() {
            Ok(mut counters) => counters.push(self.key.clone()),
            Err(poisoned) => poisoned.into_inner().push(self.key.clone()),
        }
    }

    fn absolute(&self, _value: u64) {}
}

fn stream_event_with_newlines() -> impl Strategy<Value = StreamEvent> {
    prop_oneof![
        multiline_text().prop_map(|text| StreamEvent::Comment { text }),
        multiline_text().prop_map(|raw| StreamEvent::Data {
            raw,
            parsed_json: None,
        }),
    ]
}

fn multiline_text() -> impl Strategy<Value = String> {
    proptest::collection::vec("[A-Za-z0-9 ]{0,16}", 0..8).prop_map(|lines| lines.join("\n"))
}
