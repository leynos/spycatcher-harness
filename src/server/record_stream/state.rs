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

pub(super) struct CompletedStream {
    pub(super) status: u16,
    pub(super) headers: Vec<(String, String)>,
    pub(super) raw_transcript: Vec<u8>,
    pub(super) events: Vec<StreamEvent>,
    pub(super) ttft_ms: u64,
    pub(super) chunk_offsets_ms: Vec<u64>,
}

pub(super) struct StreamRecordingState<U, E, M> {
    pub(super) service: RecordService<U, E, M>,
    pub(super) request: Option<ObservedRequest>,
    pub(super) status: u16,
    pub(super) headers: Vec<(String, String)>,
    pub(super) upstream_body: BoxStream<'static, HarnessResult<Bytes>>,
    pub(super) interaction_id: String,
    pub(super) interaction_start: Instant,
    pub(super) upstream_start: Instant,
    pub(super) parser: SseParser,
    pub(super) raw_transcript: Vec<u8>,
    pub(super) events: Vec<StreamEvent>,
    pub(super) chunk_offsets_ms: Vec<u64>,
    pub(super) ttft_ms: Option<u64>,
    pub(super) recording_failed: bool,
}

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
