//! Private CLI argument support types.
//!
//! The public CLI adapter lives in `src/cli.rs`; this module contains nested
//! serializable helper structures used by `OrthoConfig` while keeping the adapter
//! module small enough to satisfy repository health rules.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::config;

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
/// Serialisable locale and fallback-locale fields threaded through `OrthoConfig`
/// subcommand merging.
pub(super) struct LocalizationArgs {
    pub(super) locale: Option<String>,
    pub(super) fallback_locale: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
/// Serialisable upstream configuration fields for the `record` subcommand.
pub(super) struct RecordUpstreamArgs {
    #[serde(default)]
    kind: RecordUpstreamKind,
    #[serde(default = "default_record_base_url")]
    base_url: String,
    #[serde(default = "default_record_api_key_env")]
    api_key_env: String,
    #[serde(default)]
    extra_headers: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "snake_case")]
enum RecordUpstreamKind {
    #[serde(alias = "openrouter")]
    #[default]
    OpenRouter,
}

/// Returns the default `OpenRouter` API base URL.
fn default_record_base_url() -> String {
    String::from("https://openrouter.ai/api/v1")
}

/// Returns the default environment variable name used to supply the `OpenRouter`
/// API key.
fn default_record_api_key_env() -> String {
    String::from("OPENROUTER_API_KEY")
}

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
