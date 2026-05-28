//! Stream-recording helpers for record-mode chat completions.
//!
//! This module keeps long-lived response body ownership out of the ordinary
//! non-stream recording path while sharing the same cassette store and metadata
//! policies as `RecordService`.

use std::sync::atomic::Ordering;
use std::time::Instant;

use axum::body::Bytes;
use futures_util::StreamExt;
use futures_util::stream::{self, BoxStream};
use log::{error, info};

use super::{RecordError, RecordService};
use crate::cassette::{
    IgnorePathConfig, Interaction, RecordedRequest, RecordedResponse, StreamEvent, StreamTiming,
};
use crate::http_exchange::{ObservedRequest, ProxyBody, ProxyResponse, redact_headers};
use crate::protocol::{CHAT_COMPLETIONS_PROTOCOL_ID, upstream_id};
use crate::server::record_metadata::MetadataFactory;
use crate::sse::{SseParseError, SseParser};
use crate::upstream::StreamingObservedResponse;
use crate::upstream::{ChatCompletionsRequest, ChatCompletionsUpstream, EnvProvider};
use crate::{HarnessError, HarnessResult};

impl<U, E, M> RecordService<U, E, M>
where
    U: ChatCompletionsUpstream + Clone + Send + Sync + 'static,
    E: EnvProvider + Clone + Send + Sync + 'static,
    M: MetadataFactory,
{
    /// Handles a streaming chat-completions request in record mode.
    ///
    /// Calls the upstream streaming endpoint, proxies the SSE byte stream
    /// back to the client via a [`ProxyBody::Stream`], and, once the stream
    /// completes cleanly, persists the captured events and raw transcript to
    /// the cassette store.
    ///
    /// # Errors
    ///
    /// Returns [`RecordError`] if the API key cannot be resolved or if the
    /// upstream request cannot be initiated.
    pub(super) async fn handle_streaming_chat_completions(
        &self,
        request: ObservedRequest,
        interaction_start: Instant,
    ) -> Result<ProxyResponse, RecordError> {
        let api_key = self.resolve_api_key()?;
        let interaction_id = format!(
            "{proto}-{seq}",
            proto = CHAT_COMPLETIONS_PROTOCOL_ID,
            seq = self.interaction_seq.fetch_add(1, Ordering::Relaxed),
        );
        let upstream_start = Instant::now();
        let upstream_response = self
            .upstream_client
            .stream_chat_completions(ChatCompletionsRequest {
                config: &self.upstream,
                api_key: &api_key,
                headers: &request.forward_headers,
                body: &request.body,
                query: &request.query,
            })
            .await
            .map_err(|err| {
                self.failure_count.fetch_add(1, Ordering::Relaxed);
                error!(
                    target: "spycatcher.harness.record",
                    "upstream stream request failed interaction_id={interaction_id} \
                     mode=record protocol={protocol} upstream_latency_ms={upstream_latency} \
                     outcome=failed cassette={cassette} error={err}",
                    protocol = CHAT_COMPLETIONS_PROTOCOL_ID,
                    upstream_latency = upstream_start.elapsed().as_millis(),
                    cassette = upstream_id(self.upstream.kind),
                );
                RecordError::Internal
            })?;
        let status = upstream_response.status;
        let headers = upstream_response.proxy_headers.clone();
        let body = self.recording_stream(StreamingRecordStart {
            request,
            upstream_response,
            interaction_id,
            interaction_start,
            upstream_start,
        });

        Ok(ProxyResponse {
            status,
            headers,
            body: ProxyBody::Stream(body),
        })
    }

    fn recording_stream(
        &self,
        start: StreamingRecordStart,
    ) -> BoxStream<'static, HarnessResult<Bytes>> {
        let upstream_response = start.upstream_response;
        let state = StreamRecordingState {
            service: self.clone(),
            request: Some(start.request),
            status: upstream_response.status,
            headers: upstream_response.headers,
            upstream_body: upstream_response.body,
            interaction_id: start.interaction_id,
            interaction_start: start.interaction_start,
            upstream_start: start.upstream_start,
            parser: SseParser::default(),
            raw_transcript: Vec::new(),
            events: Vec::new(),
            chunk_offsets_ms: Vec::new(),
            ttft_ms: None,
            recording_failed: false,
        };
        stream::try_unfold(state, process_stream_chunk).boxed()
    }

    fn build_stream_interaction(
        &self,
        request: ObservedRequest,
        stream: CompletedStream,
        interaction_start: Instant,
    ) -> Result<Interaction, RecordError> {
        let mut recorded_request = RecordedRequest {
            method: request.method,
            path: request.path,
            query: request.query,
            headers: redact_headers(&request.headers, &self.redaction),
            body: request.body,
            parsed_json: request.parsed_json,
            canonical_request: None,
            stable_hash: None,
        };
        recorded_request
            .populate_canonical_fields(&IgnorePathConfig::default())
            .map_err(|error| {
                error!(
                    target: "spycatcher.harness.record",
                    "failed to populate canonical fields: {error}"
                );
                RecordError::Internal
            })?;
        let metadata = self
            .metadata
            .create_at(interaction_start)
            .map_err(|error| {
                error!(
                    target: "spycatcher.harness.record",
                    "failed to create interaction metadata: {error}"
                );
                RecordError::Internal
            })?;

        Ok(Interaction {
            request: recorded_request,
            response: RecordedResponse::Stream {
                status: stream.status,
                headers: redact_headers(&stream.headers, &self.redaction),
                events: stream.events,
                raw_transcript: stream.raw_transcript,
                timing: Some(StreamTiming {
                    ttft_ms: stream.ttft_ms,
                    chunk_offsets_ms: stream.chunk_offsets_ms,
                }),
            },
            metadata,
        })
    }
}

struct CompletedStream {
    status: u16,
    headers: Vec<(String, String)>,
    raw_transcript: Vec<u8>,
    events: Vec<StreamEvent>,
    ttft_ms: u64,
    chunk_offsets_ms: Vec<u64>,
}

struct StreamRecordingState<U, E, M> {
    service: RecordService<U, E, M>,
    request: Option<ObservedRequest>,
    status: u16,
    headers: Vec<(String, String)>,
    upstream_body: BoxStream<'static, HarnessResult<Bytes>>,
    interaction_id: String,
    interaction_start: Instant,
    upstream_start: Instant,
    parser: SseParser,
    raw_transcript: Vec<u8>,
    events: Vec<StreamEvent>,
    chunk_offsets_ms: Vec<u64>,
    ttft_ms: Option<u64>,
    recording_failed: bool,
}

async fn process_stream_chunk<U, E, M>(
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

struct StreamingRecordStart {
    request: ObservedRequest,
    upstream_response: StreamingObservedResponse,
    interaction_id: String,
    interaction_start: Instant,
    upstream_start: Instant,
}

async fn persist_interaction<U, E, M>(
    service: &RecordService<U, E, M>,
    interaction_id: &str,
    interaction: Interaction,
) where
    U: ChatCompletionsUpstream + Clone + Send + Sync + 'static,
    E: EnvProvider + Clone + Send + Sync + 'static,
    M: MetadataFactory,
{
    match service.append_interaction(interaction).await {
        Ok(()) => {
            service.recorded_count.fetch_add(1, Ordering::Relaxed);
            info!(
                target: "spycatcher.harness.record",
                "stream interaction recorded interaction_id={interaction_id} mode=record \
                 protocol={} outcome=recorded cassette={}",
                CHAT_COMPLETIONS_PROTOCOL_ID,
                upstream_id(service.upstream.kind),
            );
        }
        Err(error) => log_stream_write_failure(service, interaction_id, &error),
    }
}

fn log_stream_write_failure<U, E, M>(
    service: &RecordService<U, E, M>,
    interaction_id: &str,
    error: &RecordError,
) {
    service.failure_count.fetch_add(1, Ordering::Relaxed);
    error!(
        target: "spycatcher.harness.record",
        "stream cassette write failed interaction_id={interaction_id} mode=record \
         protocol={} outcome=write_failed cassette={} error={error:?}",
        CHAT_COMPLETIONS_PROTOCOL_ID,
        upstream_id(service.upstream.kind),
    );
}

fn log_stream_build_failure<U, E, M>(
    service: &RecordService<U, E, M>,
    interaction_id: &str,
    error: &RecordError,
) {
    service.failure_count.fetch_add(1, Ordering::Relaxed);
    error!(
        target: "spycatcher.harness.record",
        "stream cassette build failed interaction_id={interaction_id} mode=record \
         protocol={} outcome=build_failed cassette={} error={error:?}",
        CHAT_COMPLETIONS_PROTOCOL_ID,
        upstream_id(service.upstream.kind),
    );
}

fn stream_parse_failure(error: SseParseError) -> HarnessError {
    HarnessError::UpstreamRequestFailed {
        source: Box::new(error),
    }
}

fn millis_u64(value: u128) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}
