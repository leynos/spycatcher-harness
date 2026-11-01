//! Unit tests for replay stream serialization.

use std::sync::{Arc, Mutex};

use axum::body::to_bytes;
use metrics::{
    Counter,
    CounterFn,
    Gauge,
    Histogram,
    Key,
    KeyName,
    Metadata,
    Recorder,
    SharedString,
    Unit,
};
use proptest::prelude::*;
use rstest::rstest;

use super::*;
use crate::protocol::{CHAT_COMPLETIONS_PATH, CHAT_COMPLETIONS_PROTOCOL_ID};

#[path = "replay_stream_properties.rs"]
mod properties;

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

/// Buffered events, the threshold-crossing event, and trailing events keep order.
#[rstest]
#[tokio::test]
async fn threshold_crossing_preserves_buffered_and_remaining_events() {
    let recorder = StreamMetricRecorder::default();
    let _recorder_guard = metrics::set_default_local_recorder(&recorder);
    let first = "a".repeat(32 * 1024);
    let processing = "b".repeat(16 * 1024);
    let crossing = "c".repeat(24 * 1024);
    let prefix = format!("data: {first}\n\n: {processing}\n\n");
    let through_crossing = format!("{prefix}data: {crossing}\n\n");
    assert!(
        prefix.len() < 64 * 1024,
        "the first two events must fit inside the eager limit",
    );
    assert!(
        through_crossing.len() > 64 * 1024,
        "the third event must cross the eager limit",
    );
    let expected = format!("{through_crossing}data: [DONE]\n\n");
    let body = build_stream_body(
        vec![
            data(&first),
            comment(&processing),
            data(&crossing),
            data("[DONE]"),
        ],
        &labels(),
    );

    let bytes = to_bytes(body, 80 * 1024)
        .await
        .expect("body crossing the eager limit should remain readable");

    assert_eq!(
        bytes.as_ref(),
        expected.as_bytes(),
        "buffered, crossing and trailing events must retain their exact bytes",
    );
    assert!(
        recorder.has_counter_with_label(
            "spycatcher.replay.stream.delivery.total",
            "delivery",
            StreamDeliveryMode::Streamed.as_str(),
        ),
        "crossing the limit after buffered events should select streamed delivery",
    );
    for metric_name in [
        "spycatcher.replay.stream.responses.total",
        "spycatcher.replay.stream.delivery.total",
    ] {
        assert_eq!(
            recorder.increment_summary(
                metric_name,
                &[("delivery", "streamed"), ("outcome", "replayed")],
            ),
            (1, 1),
            "a crossing body must increment {metric_name} exactly once by one",
        );
    }
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
    counters: Arc<Mutex<Vec<(Key, u64)>>>,
}

impl StreamMetricRecorder {
    fn has_counter_with_label(
        &self,
        metric_name: &str,
        label_key: &str,
        label_value: &str,
    ) -> bool {
        self.increment_summary(metric_name, &[(label_key, label_value)])
            .0
            > 0
    }

    /// Counts matching increments and sums their values under all requested labels.
    /// For example, two increments of one return `(2, 2)`; one increment of two
    /// returns `(1, 2)`, so either accounting error fails an expected `(1, 1)`.
    fn increment_summary(
        &self,
        metric_name: &str,
        expected_labels: &[(&str, &str)],
    ) -> (usize, u64) {
        let counters = match self.counters.lock() {
            Ok(counters) => counters,
            Err(poisoned) => poisoned.into_inner(),
        };
        counters
            .iter()
            .filter(|(key, _)| {
                key.name() == metric_name
                    && expected_labels.iter().all(|(name, value)| {
                        key.labels()
                            .any(|label| label.key() == *name && label.value() == *value)
                    })
            })
            .fold((0, 0), |(count, total), (_, value)| {
                (count + 1, total + *value)
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

    fn register_gauge(&self, _key: &Key, _metadata: &Metadata<'_>) -> Gauge { Gauge::noop() }

    fn register_histogram(&self, _key: &Key, _metadata: &Metadata<'_>) -> Histogram {
        Histogram::noop()
    }
}

struct StreamMetricCounter {
    key: Key,
    counters: Arc<Mutex<Vec<(Key, u64)>>>,
}

impl CounterFn for StreamMetricCounter {
    fn increment(&self, value: u64) {
        match self.counters.lock() {
            Ok(mut counters) => counters.push((self.key.clone(), value)),
            Err(poisoned) => poisoned.into_inner().push((self.key.clone(), value)),
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
