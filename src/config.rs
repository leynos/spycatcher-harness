//! Configuration types for the Spycatcher harness.
//!
//! This module defines [`HarnessConfig`] and its constituent types.
//! All types provide sensible defaults so they can be constructed in
//! tests without specifying every field. Configuration loading via
//! `OrthoConfig` is introduced in task 1.1.2.

use std::collections::BTreeMap;
use std::net::SocketAddr;

use camino::Utf8PathBuf;

/// Top-level configuration for a harness session.
///
/// Fields correspond to the design document's `HarnessConfig` definition.
/// Use [`Default::default`] for a minimal replay-mode configuration
/// suitable for smoke tests.
///
/// # Examples
///
/// ```
/// use spycatcher_harness::HarnessConfig;
///
/// let cfg = HarnessConfig::default();
/// assert!(!cfg.cassette_name.is_empty());
/// ```
#[derive(Debug, Clone)]
pub struct HarnessConfig {
    /// Address and port the harness listens on.
    pub listen: ListenAddr,
    /// Operating mode (record or replay).
    pub mode: Mode,
    /// Localisation settings.
    pub localization: LocalizationConfig,
    /// Protocol to expose.
    pub protocol: Protocol,
    /// Request matching strategy for replay.
    pub match_mode: MatchMode,
    /// Directory containing cassette files.
    pub cassette_dir: Utf8PathBuf,
    /// Name of the cassette to record to or replay from.
    pub cassette_name: String,
    /// Upstream provider configuration (required for record mode).
    pub upstream: Option<UpstreamConfig>,
    /// Header redaction rules applied before persistence.
    pub redaction: RedactionConfig,
    /// Replay timing controls.
    pub replay: ReplayConfig,
}

impl Default for HarnessConfig {
    fn default() -> Self {
        Self {
            listen: ListenAddr::default(),
            mode: Mode::default(),
            localization: LocalizationConfig::default(),
            protocol: Protocol::default(),
            match_mode: MatchMode::default(),
            cassette_dir: Utf8PathBuf::from("fixtures/llm"),
            cassette_name: "default".to_owned(),
            upstream: None,
            redaction: RedactionConfig::default(),
            replay: ReplayConfig::default(),
        }
    }
}

/// Listen address for the harness HTTP server.
///
/// Wraps a [`SocketAddr`] with a default of `127.0.0.1:8787`.
///
/// # Examples
///
/// ```
/// use spycatcher_harness::config::ListenAddr;
///
/// let addr = ListenAddr::default();
/// assert_eq!(addr.as_socket_addr().port(), 8787);
/// ```
#[derive(Debug, Clone, Copy)]
pub struct ListenAddr(SocketAddr);

impl ListenAddr {
    /// Returns the inner [`SocketAddr`].
    #[must_use]
    pub const fn as_socket_addr(self) -> SocketAddr {
        self.0
    }
}

impl Default for ListenAddr {
    fn default() -> Self {
        Self(SocketAddr::from(([127, 0, 0, 1], 8787)))
    }
}

impl From<SocketAddr> for ListenAddr {
    fn from(addr: SocketAddr) -> Self {
        Self(addr)
    }
}

/// Operating mode for the harness.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Mode {
    /// Proxy to upstream and record interactions.
    Record,
    /// Serve responses from a recorded cassette.
    #[default]
    Replay,
}

/// Protocol exposed by the harness HTTP server.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Protocol {
    /// `OpenAI` chat completions-compatible endpoint.
    #[default]
    OpenAiChatCompletions,
}

/// Request matching strategy for replay mode.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum MatchMode {
    /// Replay expects requests in recorded order; mismatches fail fast.
    #[default]
    SequentialStrict,
    /// Match by request hash, consuming the next unused interaction.
    Keyed,
}

/// Upstream provider configuration for record mode.
///
/// Required when the harness operates in [`Mode::Record`].
#[derive(Debug, Clone)]
pub struct UpstreamConfig {
    /// Provider type.
    pub kind: UpstreamKind,
    /// Base URL for the upstream API.
    pub base_url: String,
    /// Name of the environment variable containing the API key.
    pub api_key_env: String,
    /// Additional HTTP headers sent with every upstream request.
    pub extra_headers: BTreeMap<String, String>,
}

impl Default for UpstreamConfig {
    fn default() -> Self {
        Self {
            kind: UpstreamKind::default(),
            base_url: "https://openrouter.ai/api/v1".to_owned(),
            api_key_env: "OPENROUTER_API_KEY".to_owned(),
            extra_headers: BTreeMap::new(),
        }
    }
}

/// Upstream provider type.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum UpstreamKind {
    /// `OpenRouter` API.
    #[default]
    OpenRouter,
}

/// Localisation settings for the harness.
#[derive(Debug, Clone)]
pub struct LocalizationConfig {
    /// Explicit locale override (e.g. `"en-GB"`).
    pub locale: Option<String>,
    /// Fallback locale when negotiation fails.
    pub fallback_locale: String,
}

impl Default for LocalizationConfig {
    fn default() -> Self {
        Self {
            locale: None,
            fallback_locale: "en-US".to_owned(),
        }
    }
}

/// Header redaction configuration applied before cassette persistence.
#[derive(Debug, Clone, Default)]
pub struct RedactionConfig {
    /// Header names to remove from recorded interactions.
    pub drop_headers: Vec<String>,
}

/// Replay timing controls for simulating streaming latency.
///
/// The default has `simulate_timing` disabled.  When enabled, `tps`
/// controls inter-chunk spacing; a zero value is never used for
/// division because the timing path is gated on `simulate_timing`.
#[derive(Debug, Clone)]
pub struct ReplayConfig {
    /// Whether to simulate timing delays during replay.
    pub simulate_timing: bool,
    /// Time-to-first-token delay in milliseconds.
    pub ttft_ms: u64,
    /// Tokens per second for inter-chunk spacing.
    pub tps: u64,
}

impl Default for ReplayConfig {
    fn default() -> Self {
        Self {
            simulate_timing: false,
            ttft_ms: 200,
            tps: 50,
        }
    }
}

#[cfg(test)]
mod tests {
    //! Unit tests for harness configuration defaults and validation.

    use super::*;
    use rstest::rstest;

    #[rstest]
    fn default_config_has_non_empty_cassette_name() {
        let cfg = HarnessConfig::default();
        assert!(!cfg.cassette_name.is_empty());
    }

    #[rstest]
    fn default_listen_addr_is_localhost_8787() {
        let addr = ListenAddr::default();
        assert_eq!(
            addr.as_socket_addr(),
            SocketAddr::from(([127, 0, 0, 1], 8787)),
        );
    }

    #[rstest]
    fn default_mode_is_replay() {
        assert_eq!(Mode::default(), Mode::Replay);
    }

    #[rstest]
    fn default_match_mode_is_sequential_strict() {
        assert_eq!(MatchMode::default(), MatchMode::SequentialStrict);
    }

    #[rstest]
    fn default_protocol_is_openai_chat_completions() {
        assert_eq!(Protocol::default(), Protocol::OpenAiChatCompletions);
    }

    #[rstest]
    fn default_replay_config_has_non_zero_tps() {
        let rc = ReplayConfig::default();
        assert!(
            rc.tps > 0,
            "default tps must be non-zero to avoid division-by-zero"
        );
    }

    #[rstest]
    fn listen_addr_from_socket_addr() {
        let sock = SocketAddr::from(([192, 168, 1, 1], 9090));
        let listen = ListenAddr::from(sock);
        assert_eq!(listen.as_socket_addr(), sock);
    }
}
