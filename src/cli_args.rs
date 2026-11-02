//! Private CLI argument support types.
//!
//! The public CLI adapter lives in `src/cli/mod.rs`; this module contains
//! nested serializable helper structures used by `OrthoConfig` while keeping
//! the adapter module small enough to satisfy repository health rules.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use url::Url;

use crate::config;

/// Serializable locale and fallback-locale fields threaded through `OrthoConfig`
/// subcommand merging.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub(super) struct LocalizationArgs {
    /// Optional requested locale override applied to the merged configuration.
    pub(super) locale: Option<String>,
    /// Optional fallback locale override applied to the merged configuration.
    pub(super) fallback_locale: Option<String>,
}

/// Serializable upstream configuration fields for the `record` subcommand.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(super) struct RecordUpstreamArgs {
    /// Upstream provider used by the `record` command.
    #[serde(default)]
    kind: RecordUpstreamKind,
    /// Base URL for the upstream API.
    #[serde(default = "default_record_base_url")]
    base_url: Url,
    /// Name of the environment variable that supplies the upstream API key.
    #[serde(default = "default_record_api_key_env")]
    api_key_env: String,
    /// Additional HTTP headers sent with upstream requests.
    #[serde(default)]
    extra_headers: BTreeMap<String, String>,
}

impl Default for RecordUpstreamArgs {
    fn default() -> Self {
        Self {
            kind: RecordUpstreamKind::default(),
            base_url: default_record_base_url(),
            api_key_env: default_record_api_key_env(),
            extra_headers: BTreeMap::new(),
        }
    }
}

/// Upstream provider configured for recording.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "snake_case")]
enum RecordUpstreamKind {
    /// `OpenRouter`'s chat-completions API.
    #[serde(alias = "openrouter")]
    #[default]
    OpenRouter,
}

/// Returns the default `OpenRouter` API base URL.
fn default_record_base_url() -> Url {
    match Url::parse("https://openrouter.ai/api/v1") {
        Ok(url) => url,
        Err(error) => panic!("hardcoded base URL is invalid: {error}"),
    }
}

/// Returns the default environment variable name used to supply the `OpenRouter`
/// API key.
fn default_record_api_key_env() -> String { String::from("OPENROUTER_API_KEY") }

impl From<RecordUpstreamArgs> for config::UpstreamConfig {
    fn from(value: RecordUpstreamArgs) -> Self {
        Self {
            kind: value.kind.into(),
            base_url: value.base_url,
            api_key_env: value.api_key_env,
            extra_headers: value.extra_headers,
        }
    }
}

impl From<RecordUpstreamKind> for config::UpstreamKind {
    fn from(value: RecordUpstreamKind) -> Self {
        match value {
            RecordUpstreamKind::OpenRouter => Self::OpenRouter,
        }
    }
}
