//! Deterministic request canonicalization and stable hashing helpers.
//!
//! This module owns the pure domain logic used to normalise recorded
//! requests before matching. It deliberately avoids filesystem, CLI, and
//! transport dependencies so the same canonical form can be reused by future
//! record and replay adapters.

mod hex;
mod json;
mod query;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use thiserror::Error;

use super::RecordedRequest;
use hex::encode_hex;
use json::{canonical_request_value, canonicalize_body, serialize_json_canonical};
use query::canonicalize_query;

/// Configuration for request-body fields ignored during canonicalization.
///
/// Paths use JSON Pointer syntax from RFC 6901. Matching fields are removed
/// from the parsed JSON body before stable serialization.
///
/// # Examples
///
/// ```
/// use spycatcher_harness::cassette::IgnorePathConfig;
///
/// let config = IgnorePathConfig {
///     ignored_body_paths: vec!["/metadata/run_id".to_owned()],
/// };
/// assert_eq!(config.ignored_body_paths, vec!["/metadata/run_id"]);
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IgnorePathConfig {
    /// JSON Pointer paths to remove from the request body before hashing.
    pub ignored_body_paths: Vec<String>,
}

/// Errors raised while canonicalizing recorded requests.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum CanonicalError {
    /// A configured ignore path is not a valid JSON Pointer.
    #[error("invalid JSON Pointer path: {0:?}")]
    InvalidPointerPath(String),
}

/// A request reduced to a deterministic shape for matching and hashing.
///
/// # Examples
///
/// ```
/// use serde_json::json;
/// use spycatcher_harness::cassette::CanonicalRequest;
///
/// let canonical = CanonicalRequest {
///     method: "POST".to_owned(),
///     path: "/v1/chat/completions".to_owned(),
///     canonical_query: "a=1&b=2".to_owned(),
///     canonical_body: Some(json!({"model": "gpt-test"})),
/// };
/// assert_eq!(canonical.method, "POST");
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CanonicalRequest {
    /// HTTP method in uppercase.
    pub method: String,
    /// Request path without query string.
    pub path: String,
    /// Query parameters sorted by key then value and re-encoded.
    pub canonical_query: String,
    /// Canonical JSON body with sorted keys and ignored paths removed.
    pub canonical_body: Option<Value>,
}

impl RecordedRequest {
    /// Computes and stores the canonical request and stable hash fields.
    ///
    /// # Errors
    ///
    /// Returns [`CanonicalError::InvalidPointerPath`] when any configured
    /// ignore path is not a valid RFC 6901 JSON Pointer.
    ///
    /// # Examples
    ///
    /// ```
    /// use serde_json::json;
    /// use spycatcher_harness::cassette::{IgnorePathConfig, RecordedRequest};
    ///
    /// # fn example() -> Result<(), spycatcher_harness::cassette::CanonicalError> {
    /// let mut request = RecordedRequest {
    ///     method: "post".to_owned(),
    ///     path: "/v1/chat/completions".to_owned(),
    ///     query: "b=2&a=1".to_owned(),
    ///     headers: Vec::new(),
    ///     body: br#"{"metadata":{"run_id":"42"},"model":"gpt-test"}"#.to_vec(),
    ///     parsed_json: Some(json!({"metadata": {"run_id": "42"}, "model": "gpt-test"})),
    ///     canonical_request: None,
    ///     stable_hash: None,
    /// };
    ///
    /// request.populate_canonical_fields(&IgnorePathConfig {
    ///     ignored_body_paths: vec!["/metadata/run_id".to_owned()],
    /// })?;
    ///
    /// assert!(request.canonical_request.is_some());
    /// assert!(request.stable_hash.is_some());
    /// # Ok(())
    /// # }
    /// ```
    pub fn populate_canonical_fields(
        &mut self,
        ignore_config: &IgnorePathConfig,
    ) -> Result<(), CanonicalError> {
        let canonical = canonicalize(self, ignore_config)?;
        self.stable_hash = Some(stable_hash(&canonical));
        self.canonical_request = Some(canonical_request_value(&canonical));
        Ok(())
    }
}

/// Canonicalizes a recorded request into a deterministic representation.
///
/// # Errors
///
/// Returns [`CanonicalError::InvalidPointerPath`] when any configured ignore
/// path is not a valid RFC 6901 JSON Pointer.
///
/// # Examples
///
/// ```
/// use serde_json::json;
/// use spycatcher_harness::cassette::{IgnorePathConfig, RecordedRequest, canonicalize};
///
/// # fn example() -> Result<(), spycatcher_harness::cassette::CanonicalError> {
/// let request = RecordedRequest {
///     method: "post".to_owned(),
///     path: "/v1/chat/completions".to_owned(),
///     query: "b=2&a=1".to_owned(),
///     headers: Vec::new(),
///     body: br#"{"metadata":{"run_id":"42"},"model":"gpt-test"}"#.to_vec(),
///     parsed_json: Some(json!({"metadata": {"run_id": "42"}, "model": "gpt-test"})),
///     canonical_request: None,
///     stable_hash: None,
/// };
///
/// let canonical = canonicalize(
///     &request,
///     &IgnorePathConfig {
///         ignored_body_paths: vec!["/metadata/run_id".to_owned()],
///     },
/// )?;
///
/// assert_eq!(canonical.method, "POST");
/// assert_eq!(canonical.canonical_query, "a=1&b=2");
/// # Ok(())
/// # }
/// ```
pub fn canonicalize(
    request: &RecordedRequest,
    ignore_config: &IgnorePathConfig,
) -> Result<CanonicalRequest, CanonicalError> {
    validate_ignore_paths(ignore_config)?;

    Ok(CanonicalRequest {
        method: request.method.to_ascii_uppercase(),
        path: request.path.clone(),
        canonical_query: canonicalize_query(&request.query),
        canonical_body: request
            .parsed_json
            .clone()
            .map(|value| canonicalize_body(value, &ignore_config.ignored_body_paths))
            .transpose()?,
    })
}

fn validate_ignore_paths(ignore_config: &IgnorePathConfig) -> Result<(), CanonicalError> {
    for path in &ignore_config.ignored_body_paths {
        if !is_valid_ignore_path(path) {
            return Err(CanonicalError::InvalidPointerPath(path.clone()));
        }
    }

    Ok(())
}

fn is_valid_ignore_path(path: &str) -> bool {
    !path.is_empty() && path.starts_with('/') && json::is_valid_json_pointer(path)
}

/// Computes the stable SHA-256 hash for a canonical request.
///
/// The hash input is the UTF-8 byte string:
/// `METHOD\n{method}\nPATH\n{path}\nQUERY\n{query}\nBODY\n{body}`.
/// The body portion is empty when the request is not JSON.
///
/// # Examples
///
/// ```
/// use serde_json::json;
/// use spycatcher_harness::cassette::{CanonicalRequest, stable_hash};
///
/// let canonical = CanonicalRequest {
///     method: "POST".to_owned(),
///     path: "/v1/chat/completions".to_owned(),
///     canonical_query: "a=1&b=2".to_owned(),
///     canonical_body: Some(json!({"model": "gpt-test"})),
/// };
///
/// let hash = stable_hash(&canonical);
/// assert_eq!(hash.len(), 64);
/// ```
#[must_use]
pub fn stable_hash(canonical: &CanonicalRequest) -> String {
    let mut hasher = Sha256::new();
    hasher.update("METHOD\n");
    hasher.update(&canonical.method);
    hasher.update("\nPATH\n");
    hasher.update(&canonical.path);
    hasher.update("\nQUERY\n");
    hasher.update(&canonical.canonical_query);
    hasher.update("\nBODY\n");
    if let Some(body) = &canonical.canonical_body {
        hasher.update(serialize_json_canonical(body));
    }
    let digest = hasher.finalize();
    encode_hex(digest.as_slice())
}
