//! State and chunk processing for recorded streaming responses.

use std::sync::atomic::Ordering;
use std::time::Instant;

use axum::body::Bytes;
use futures_util::StreamExt;
use futures_util::stream::BoxStream;
use log::error;

use super::{
    RecordService, log_stream_build_failure, millis_u64, persist_interaction, stream_parse_failure,
};
use crate::HarnessResult;
use crate::cassette::StreamEvent;
use crate::http_exchange::ObservedRequest;
use crate::protocol::{CHAT_COMPLETIONS_PROTOCOL_ID, upstream_id};
use crate::server::record_metadata::MetadataFactory;
use crate::sse::SseParser;
use crate::upstream::{ChatCompletionsUpstream, EnvProvider};

/// Holds the parsed and raw data needed to build a completed stream recording.
pub(super) struct CompletedStream {
    /// HTTP status returned by the upstream service.
    pub(super) status: u16,
    /// Response headers retained with the recording.
    pub(super) headers: Vec<(String, String)>,
    /// Concatenated bytes received from the upstream stream.
    pub(super) raw_transcript: Vec<u8>,
    /// Events parsed from the upstream server-sent event stream.
    pub(super) events: Vec<StreamEvent>,
    /// Milliseconds from the upstream start until its first received chunk, or zero if none
    /// arrived.
    pub(super) ttft_ms: u64,
    /// Elapsed milliseconds for each parsed event, in the same order as `events`.
    pub(super) chunk_offsets_ms: Vec<u64>,
}

/// Tracks response data while an upstream stream is consumed and recorded.
pub(super) struct StreamRecordingState<U, E, M> {
    /// Service dependencies used to build and persist the completed interaction.
    pub(super) service: RecordService<U, E, M>,
    /// Request to include in the recording; consumed when the recording is built.
    pub(super) request: Option<ObservedRequest>,
    /// HTTP status returned by the upstream service.
    pub(super) status: u16,
    /// Response headers retained with the recording.
    pub(super) headers: Vec<(String, String)>,
    /// Remaining upstream response chunks, or an error from the upstream stream.
    pub(super) upstream_body: BoxStream<'static, HarnessResult<Bytes>>,
    /// Identifier used to correlate stream diagnostics and logs.
    pub(super) interaction_id: String,
    /// Start time of the interaction, used when constructing its recording.
    pub(super) interaction_start: Instant,
    /// Start time used to measure the first chunk and event offsets.
    pub(super) upstream_start: Instant,
    /// Incremental parser for the upstream server-sent event stream.
    pub(super) parser: SseParser,
    /// Upstream bytes collected so far, in their original order.
    pub(super) raw_transcript: Vec<u8>,
    /// Events parsed from the chunks observed so far.
    pub(super) events: Vec<StreamEvent>,
    /// Elapsed milliseconds for parsed events, in the same order as `events`.
    pub(super) chunk_offsets_ms: Vec<u64>,
    /// Elapsed milliseconds to the first received chunk, if one has arrived.
    pub(super) ttft_ms: Option<u64>,
    /// Whether parsing failed and the partial stream must not be persisted.
    pub(super) recording_failed: bool,
}

/// Returns one upstream response chunk and updated recording state at a time.
///
/// When the upstream stream ends, the state is finalized and the function returns `Ok(None)`.
/// For example, two successful upstream chunk results produce two
/// `Ok(Some((chunk, state)))` results as the returned state is passed into each
/// subsequent call, followed by `Ok(None)` after the recording has been finalized.
pub(super) async fn process_stream_chunk<U, E, M>(
    mut state: StreamRecordingState<U, E, M>,
) -> HarnessResult<Option<(Bytes, StreamRecordingState<U, E, M>)>>
where
    U: ChatCompletionsUpstream + Clone + Send + Sync + 'static,
    E: EnvProvider + Clone + Send + Sync + 'static,
    M: MetadataFactory,
{
    let Some(chunk_result) = state.upstream_body.next().await else {
        state.finish_recording().await;
        return Ok(None);
    };
    let chunk = chunk_result.inspect_err(|error| {
        state.service.failure_count.fetch_add(1, Ordering::Relaxed);
        error!(
            target: "spycatcher.harness.record",
            "upstream stream chunk failed interaction_id={} mode=record \
             protocol={} outcome=stream_failed cassette={} error={error}",
            state.interaction_id,
            CHAT_COMPLETIONS_PROTOCOL_ID,
            upstream_id(state.service.upstream.kind),
        );
    })?;
    state.observe_chunk(&chunk);
    Ok(Some((chunk, state)))
}

impl<U, E, M> StreamRecordingState<U, E, M>
where
    U: ChatCompletionsUpstream + Clone + Send + Sync + 'static,
    E: EnvProvider + Clone + Send + Sync + 'static,
    M: MetadataFactory,
{
    /// Adds a received chunk to the transcript and records events parsed from it.
    ///
    /// For example, a chunk containing one complete event adds its bytes to `raw_transcript`,
    /// appends that event to `events`, and records its elapsed offset in `chunk_offsets_ms`.
    fn observe_chunk(&mut self, chunk: &Bytes) {
        if self.recording_failed {
            return;
        }
        let elapsed = millis_u64(self.upstream_start.elapsed().as_millis());
        self.ttft_ms.get_or_insert(elapsed);
        self.raw_transcript.extend_from_slice(chunk);
        let new_events = match self.parser.feed(chunk) {
            Ok(events) => events,
            Err(parse_error) => {
                self.recording_failed = true;
                self.service.failure_count.fetch_add(1, Ordering::Relaxed);
                let error = stream_parse_failure(parse_error);
                error!(
                    target: "spycatcher.harness.record",
                    "upstream stream parse failed interaction_id={} mode=record \
                     protocol={} outcome=parse_failed cassette={} error={error}",
                    self.interaction_id,
                    CHAT_COMPLETIONS_PROTOCOL_ID,
                    upstream_id(self.service.upstream.kind),
                );
                return;
            }
        };
        self.chunk_offsets_ms
            .extend(std::iter::repeat_n(elapsed, new_events.len()));
        self.events.extend(new_events);
    }

    /// Finishes parsing the stream and persists it when parsing succeeds.
    ///
    /// For example, when the upstream stream ends with a complete final event, this method adds
    /// that event and attempts to build and persist the completed recording. A malformed final
    /// event increments the failure count and is not persisted.
    async fn finish_recording(mut self) {
        if self.recording_failed {
            return;
        }
        match self.parser.finish() {
            Ok(new_events) => {
                let elapsed = millis_u64(self.upstream_start.elapsed().as_millis());
                self.chunk_offsets_ms
                    .extend(std::iter::repeat_n(elapsed, new_events.len()));
                self.events.extend(new_events);
                self.append_completed_recording().await;
            }
            Err(parse_error) => {
                self.service.failure_count.fetch_add(1, Ordering::Relaxed);
                let error = stream_parse_failure(parse_error);
                error!(
                    target: "spycatcher.harness.record",
                    "upstream stream parse failed interaction_id={} mode=record \
                     protocol={} outcome=parse_failed cassette={} error={error}",
                    self.interaction_id,
                    CHAT_COMPLETIONS_PROTOCOL_ID,
                    upstream_id(self.service.upstream.kind),
                );
            }
        }
    }

    /// Builds and persists the completed interaction, recording failures if construction fails.
    ///
    /// For example, with a request still present, the method combines it with the accumulated
    /// stream data and persists the resulting interaction. If the request was already consumed,
    /// the method records a failure and returns without persisting.
    async fn append_completed_recording(mut self) {
        let Some(request) = self.request.take() else {
            self.service.failure_count.fetch_add(1, Ordering::Relaxed);
            error!(
                target: "spycatcher.harness.record",
                "stream cassette build failed interaction_id={} mode=record \
                 protocol={} outcome=request_missing cassette={} error=request already taken",
                self.interaction_id,
                CHAT_COMPLETIONS_PROTOCOL_ID,
                upstream_id(self.service.upstream.kind),
            );
            return;
        };
        let service = self.service.clone();
        let interaction_id = self.interaction_id.clone();
        let completed = CompletedStream {
            status: self.status,
            headers: self.headers,
            raw_transcript: self.raw_transcript,
            events: self.events,
            ttft_ms: self.ttft_ms.unwrap_or(0),
            chunk_offsets_ms: self.chunk_offsets_ms,
        };
        match self
            .service
            .build_stream_interaction(request, completed, self.interaction_start)
        {
            Ok(interaction) => persist_interaction(&service, &interaction_id, interaction).await,
            Err(error) => log_stream_build_failure(&service, &interaction_id, &error),
        }
    }
}
