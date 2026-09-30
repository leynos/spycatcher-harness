//! Shared configuration-snapshot adapter for isolated CLI test targets.

use super::isolated_cli_process::{ConfigSnapshot, ProbeOutcome, ProbeRequest, run_probe};

/// Loads the CLI configuration in a child process with controlled inputs.
///
/// # Example
///
/// ```ignore
/// let config = load_config_snapshot(&["spycatcher-harness", "replay"], None, &[])?;
/// // Returns the merged replay configuration, or its user-facing error.
/// ```
pub(super) fn load_config_snapshot(
    args: &[&str],
    config_file: Option<&str>,
    env_vars: &[(&str, &str)],
) -> Result<ConfigSnapshot, String> {
    let request = ProbeRequest::LoadConfig {
        args: args.iter().map(|arg| (*arg).to_owned()).collect(),
    };
    let envelope = run_probe(&request, config_file, env_vars)?;
    match envelope.outcome {
        ProbeOutcome::Loaded(config) => Ok(config),
        ProbeOutcome::ConfigError { message, .. } => Err(message),
        _ => Err(String::from(
            "configuration probe returned an unexpected outcome",
        )),
    }
}
