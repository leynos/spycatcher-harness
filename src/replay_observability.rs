//! Shared metrics labels and emitters for replay-mode observability.
//!
//! The library emits metrics through the `metrics` facade only. Applications
//! remain responsible for installing a recorder and exporting the measurements.

use metrics::{counter, histogram};

/// Stable replay mode label used by spans and metrics.
pub(crate) const MODE_REPLAY: &str = "replay";

/// Bounded labels shared by replay-mode metrics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReplayMetricLabels {
    /// Configured cassette name.
    pub(crate) cassette: String,
    /// Stable protocol identifier.
    pub(crate) protocol: &'static str,
    /// Stable route path.
    pub(crate) route: &'static str,
}

impl ReplayMetricLabels {
    /// Creates a replay metric label set.
    #[must_use]
    pub(crate) const fn new(cassette: String, protocol: &'static str, route: &'static str) -> Self {
        Self {
            cassette,
            protocol,
            route,
        }
    }
}

/// Replay stream body delivery modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StreamDeliveryMode {
    /// Stream content fits inside the eager body buffer.
    Eager,
    /// Stream content exceeds the eager limit and is delivered incrementally.
    Streamed,
}

impl StreamDeliveryMode {
    /// Returns the bounded metric label for this delivery mode.
    #[must_use]
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Eager => "eager",
            Self::Streamed => "streamed",
        }
    }
}

/// Records a replay request outcome.
pub(crate) fn record_replay_request(labels: &ReplayMetricLabels, outcome: &'static str) {
    counter!(
        "spycatcher.replay.requests.total",
        "cassette" => labels.cassette.clone(),
        "protocol" => labels.protocol,
        "route" => labels.route,
        "outcome" => outcome,
    )
    .increment(1);
}

/// Records a replay request mismatch.
pub(crate) fn record_replay_mismatch(labels: &ReplayMetricLabels, reason: &'static str) {
    counter!(
        "spycatcher.replay.mismatches.total",
        "cassette" => labels.cassette.clone(),
        "protocol" => labels.protocol,
        "route" => labels.route,
        "outcome" => "mismatch",
        "reason" => reason,
    )
    .increment(1);
}

/// Records a replay request rejection.
pub(crate) fn record_replay_rejection(labels: &ReplayMetricLabels, outcome: &'static str) {
    counter!(
        "spycatcher.replay.rejections.total",
        "cassette" => labels.cassette.clone(),
        "protocol" => labels.protocol,
        "route" => labels.route,
        "outcome" => outcome,
    )
    .increment(1);
}

/// Records a replay match commit failure.
pub(crate) fn record_replay_commit_failure(labels: &ReplayMetricLabels) {
    counter!(
        "spycatcher.replay.commit_failures.total",
        "cassette" => labels.cassette.clone(),
        "protocol" => labels.protocol,
        "route" => labels.route,
        "outcome" => "commit_failure",
    )
    .increment(1);
}

/// Records replay stream delivery metrics.
pub(crate) fn record_stream_delivery(
    labels: &ReplayMetricLabels,
    delivery: StreamDeliveryMode,
    event_count: usize,
) {
    let delivery_label = delivery.as_str();
    counter!(
        "spycatcher.replay.stream.responses.total",
        "cassette" => labels.cassette.clone(),
        "protocol" => labels.protocol,
        "route" => labels.route,
        "delivery" => delivery_label,
        "outcome" => "replayed",
    )
    .increment(1);
    counter!(
        "spycatcher.replay.stream.delivery.total",
        "cassette" => labels.cassette.clone(),
        "protocol" => labels.protocol,
        "route" => labels.route,
        "delivery" => delivery_label,
        "outcome" => "replayed",
    )
    .increment(1);
    histogram!(
        "spycatcher.replay.stream.events",
        "cassette" => labels.cassette.clone(),
        "protocol" => labels.protocol,
        "route" => labels.route,
        "delivery" => delivery_label,
        "outcome" => "replayed",
    )
    .record(bounded_event_count(event_count));
}

fn bounded_event_count(event_count: usize) -> u32 {
    u32::try_from(event_count).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    //! Unit tests for replay observability helpers.

    use super::*;

    #[rstest::rstest]
    #[case(StreamDeliveryMode::Eager, "eager")]
    #[case(StreamDeliveryMode::Streamed, "streamed")]
    fn delivery_mode_labels_are_bounded(
        #[case] delivery: StreamDeliveryMode,
        #[case] expected: &str,
    ) {
        assert_eq!(delivery.as_str(), expected);
    }

    #[rstest::rstest]
    fn event_count_uses_bounded_u32_histogram_value() {
        assert_eq!(bounded_event_count(42), 42);
    }
}
