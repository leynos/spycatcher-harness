//! Child-process probes for configuration and environment-backed CLI policy.
//!
//! The probes keep environment values and working-directory discovery inputs
//! out of the parent test process while exercising the real CLI adapter.

use std::io::{self, Write};
use std::process::Command;
use std::sync::{Arc, Mutex};

use cap_std::fs::Dir;
use serde::{Deserialize, Serialize};
use spycatcher_harness::cli::CliConfigError;
use spycatcher_harness::cli::load_subcommand_config_from_iter;
use spycatcher_harness::cli::localizer::{
    early_locale_plan as select_early_locale_plan, is_cli_localization_disabled,
};
use spycatcher_harness::config::{HarnessConfig, Mode};
use tempfile::TempDir;
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::fmt::format::FmtSpan;

const REQUEST_ENV: &str = "SPYCATCHER_HARNESS_TEST_PROBE_REQUEST";
const RESPONSE_PREFIX: &str = "SPYCATCHER_HARNESS_TEST_PROBE_RESPONSE=";

#[derive(Debug, Serialize, Deserialize)]
pub(super) enum ProbeRequest {
    LoadConfig { args: Vec<String> },
    EarlyLocalePlan,
    LocalizationDisabled,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub(super) enum ProbeOutcome {
    Loaded(ConfigSnapshot),
    ConfigError { kind: String, message: String },
    LocalePlan(String),
    LocalizationDisabled(bool),
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub(super) struct ProbeEnvelope {
    pub(super) outcome: ProbeOutcome,
    pub(super) tracing_output: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(super) struct ConfigSnapshot {
    pub(super) listen: String,
    pub(super) mode: ModeSnapshot,
    pub(super) cassette_dir: String,
    pub(super) cassette_name: String,
    pub(super) localization: LocalizationSnapshot,
    pub(super) upstream: Option<UpstreamSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(super) struct LocalizationSnapshot {
    pub(super) locale: Option<String>,
    pub(super) fallback_locale: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum ModeSnapshot {
    Record,
    Replay,
    Verify,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(super) struct UpstreamSnapshot {
    pub(super) base_url: String,
    pub(super) api_key_env: String,
}

/// Runs one CLI-policy probe with isolated files and environment values.
///
/// # Examples
///
/// ```ignore
/// let response = run_probe(&ProbeRequest::EarlyLocalePlan, None, &[])?;
/// // The child returns the selected early locale plan.
/// ```
pub(super) fn run_probe(
    request: &ProbeRequest,
    config_file: Option<&str>,
    env_vars: &[(&str, &str)],
) -> Result<ProbeEnvelope, String> {
    let work_dir =
        tempfile::tempdir().map_err(|_| String::from("could not create probe directory"))?;
    if let Some(file_content) = config_file {
        write_config_file(&work_dir, file_content)?;
    }

    let request_json = serde_json::to_string(request)
        .map_err(|_| String::from("could not encode child probe request"))?;
    let output = run_child(&work_dir, &request_json, env_vars)?;
    parse_probe_output(output, env_vars)
}

fn write_config_file(work_dir: &TempDir, content: &str) -> Result<(), String> {
    let directory = Dir::open_ambient_dir(work_dir.path(), cap_std::ambient_authority())
        .map_err(|_| String::from("could not open probe directory"))?;
    directory
        .write(".spycatcher_harness.toml", content.as_bytes())
        .map_err(|_| String::from("could not write probe config file"))
}

#[tracing::instrument(skip_all)]
fn run_child(
    work_dir: &TempDir,
    request: &str,
    env_vars: &[(&str, &str)],
) -> Result<std::process::Output, String> {
    let executable = std::env::current_exe()
        .map_err(|_| String::from("could not locate probe test executable"))?;
    let mut command = Command::new(executable);
    command
        .args([
            "--exact",
            "isolated_cli_process::probe_child",
            "--nocapture",
        ])
        .current_dir(work_dir.path())
        .env_clear()
        .env(REQUEST_ENV, request)
        .env("HOME", work_dir.path())
        .env("USERPROFILE", work_dir.path())
        .env("XDG_CONFIG_HOME", work_dir.path())
        .env("XDG_CONFIG_DIRS", work_dir.path())
        .env("APPDATA", work_dir.path())
        .env("LOCALAPPDATA", work_dir.path())
        .env("TMP", work_dir.path())
        .env("TEMP", work_dir.path())
        .env("TMPDIR", work_dir.path());

    // Forward only LLVM's profile destination so child probes are counted in
    // coverage while configuration and other environment values stay cleared.
    if let Some(profile_file) = std::env::var_os("LLVM_PROFILE_FILE") {
        command.env("LLVM_PROFILE_FILE", profile_file);
    }

    for name in [
        "PATH",
        "SystemRoot",
        "SYSTEMROOT",
        "WINDIR",
        "LD_LIBRARY_PATH",
        "DYLD_LIBRARY_PATH",
    ] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    for (name, value) in env_vars {
        command.env(name, value);
    }

    command
        .output()
        .map_err(|_| String::from("could not run isolated CLI probe"))
}

fn parse_probe_output(
    output: std::process::Output,
    env_vars: &[(&str, &str)],
) -> Result<ProbeEnvelope, String> {
    let stderr_tail = bounded_stderr_tail(&output.stderr, env_vars);
    let stderr_detail = if stderr_tail.is_empty() {
        String::new()
    } else {
        format!(": stderr tail: {stderr_tail}")
    };
    if !output.status.success() {
        return Err(format!(
            "isolated CLI probe exited with {}{stderr_detail}",
            output.status,
        ));
    }

    let stdout = String::from_utf8(output.stdout)
        .map_err(|_| String::from("isolated CLI probe returned non-UTF-8 output"))?;
    let encoded = stdout
        .lines()
        .find_map(|line| line.strip_prefix(RESPONSE_PREFIX))
        .ok_or_else(|| format!("isolated CLI probe returned no response{stderr_detail}"))?;
    serde_json::from_str(encoded).map_err(|_| String::from("could not decode child probe response"))
}

/// Redacts supplied environment values and returns a bounded UTF-8 stderr tail.
///
/// # Examples
///
/// ```ignore
/// let tail = bounded_stderr_tail(b"probe failed", &[]);
/// assert_eq!(tail, "probe failed");
/// ```
pub(super) fn bounded_stderr_tail(stderr: &[u8], env_vars: &[(&str, &str)]) -> String {
    const MAX_TAIL_BYTES: usize = 2048;
    const TAIL_MARKER: &str = "…";
    let mut sanitized = String::from_utf8_lossy(stderr).into_owned();
    for (_, value) in env_vars.iter().filter(|(_, value)| !value.is_empty()) {
        sanitized = sanitized.replace(value, "[redacted]");
    }
    if sanitized.len() <= MAX_TAIL_BYTES {
        return sanitized;
    }

    let max_tail_bytes = MAX_TAIL_BYTES.saturating_sub(TAIL_MARKER.len());
    let mut tail_start = sanitized.len().saturating_sub(max_tail_bytes);
    while !sanitized.is_char_boundary(tail_start) {
        tail_start = tail_start.saturating_add(1);
    }
    let tail = sanitized.get(tail_start..).unwrap_or_default();
    format!("{TAIL_MARKER}{tail}")
}

#[tracing::instrument(skip_all)]
fn handle_probe(request: ProbeRequest) -> ProbeOutcome {
    match request {
        ProbeRequest::LoadConfig { args } => match load_subcommand_config_from_iter(args) {
            Ok(config) => ProbeOutcome::Loaded(snapshot_config(config)),
            Err(error) => ProbeOutcome::ConfigError {
                kind: config_error_kind(&error).to_owned(),
                message: error.to_string(),
            },
        },
        ProbeRequest::EarlyLocalePlan => {
            ProbeOutcome::LocalePlan(select_early_locale_plan().to_string())
        }
        ProbeRequest::LocalizationDisabled => {
            ProbeOutcome::LocalizationDisabled(is_cli_localization_disabled())
        }
    }
}

const fn config_error_kind(error: &CliConfigError) -> &'static str {
    match error {
        CliConfigError::CliParse(_) => "cli_parse",
        CliConfigError::Merge { .. } => "merge",
        CliConfigError::InvalidLocale { .. } => "invalid_locale",
        CliConfigError::DisplayRequested { .. } => "display",
    }
}

fn snapshot_config(config: HarnessConfig) -> ConfigSnapshot {
    let upstream = config.upstream.map(|upstream| UpstreamSnapshot {
        base_url: upstream.base_url.to_string(),
        api_key_env: upstream.api_key_env,
    });

    ConfigSnapshot {
        listen: config.listen.as_socket_addr().to_string(),
        mode: match config.mode {
            Mode::Record => ModeSnapshot::Record,
            Mode::Replay => ModeSnapshot::Replay,
            Mode::Verify => ModeSnapshot::Verify,
        },
        cassette_dir: config.cassette_dir.to_string(),
        cassette_name: config.cassette_name,
        localization: LocalizationSnapshot {
            locale: config.localization.locale,
            fallback_locale: config.localization.fallback_locale,
        },
        upstream,
    }
}

#[derive(Clone)]
struct CapturedWriter(Arc<Mutex<Vec<u8>>>);

impl io::Write for CapturedWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let mut output = self
            .0
            .lock()
            .map_err(|error| io::Error::other(error.to_string()))?;
        output.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<'writer> MakeWriter<'writer> for CapturedWriter {
    type Writer = Self;

    fn make_writer(&'writer self) -> Self::Writer {
        self.clone()
    }
}

fn captured_tracing(output: &Arc<Mutex<Vec<u8>>>) -> String {
    output
        .lock()
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
        .unwrap_or_default()
}

#[test]
fn probe_child() {
    let Ok(request_json) = std::env::var(REQUEST_ENV) else {
        return;
    };
    let probe_request: ProbeRequest =
        serde_json::from_str(&request_json).expect("parent should provide a valid probe request");
    let trace_bytes = Arc::new(Mutex::new(Vec::new()));
    let subscriber = tracing_subscriber::fmt()
        .without_time()
        .with_ansi(false)
        .with_max_level(tracing::Level::TRACE)
        .with_span_events(FmtSpan::NEW)
        .with_writer(CapturedWriter(Arc::clone(&trace_bytes)))
        .finish();
    let outcome = tracing::subscriber::with_default(subscriber, || handle_probe(probe_request));
    let response = ProbeEnvelope {
        outcome,
        tracing_output: captured_tracing(&trace_bytes),
    };
    let encoded = serde_json::to_string(&response).expect("probe response should be serializable");
    let mut stdout = io::stdout().lock();
    writeln!(stdout, "\n{RESPONSE_PREFIX}{encoded}").expect("probe response should be writable");
}
