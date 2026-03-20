//! Filesystem adapter for cassette persistence.
//!
//! This module keeps file I/O out of the domain-owned cassette schema while
//! providing append-only record-mode writes and read-only replay loads.

use camino::Utf8Path;
use cap_std::{ambient_authority, fs_utf8::Dir};

use crate::cassette::{Cassette, CassetteAppender, CassetteReader, Interaction};
use crate::{HarnessError, HarnessResult};

/// Filesystem-backed cassette store used by record and replay startup paths.
#[derive(Debug)]
pub(crate) struct FilesystemCassetteStore {
    parent_dir: Dir,
    file_name: String,
    cassette: Cassette,
}

impl FilesystemCassetteStore {
    /// Opens an existing cassette for replay using read-only access.
    ///
    /// # Errors
    ///
    /// Returns a harness error when the cassette cannot be opened or decoded.
    pub(crate) fn open_for_replay(cassette_path: &Utf8Path) -> HarnessResult<Self> {
        Self::open_existing(cassette_path)
    }

    /// Opens an existing cassette for record mode, or creates an empty one.
    ///
    /// # Errors
    ///
    /// Returns a harness error when the cassette cannot be created, opened,
    /// or decoded.
    pub(crate) fn open_or_create_for_record(cassette_path: &Utf8Path) -> HarnessResult<Self> {
        let (parent_dir, file_name) = open_rooted_parent(cassette_path, true)?;
        let (cassette, is_new) = match parent_dir.open(&file_name) {
            Ok(file) => (Cassette::from_reader(file)?, false),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (Cassette::new(), true),
            Err(error) => return Err(HarnessError::from(error)),
        };
        let mut store = Self {
            parent_dir,
            file_name,
            cassette,
        };
        if is_new {
            store.flush()?;
        }
        Ok(store)
    }

    fn open_existing(cassette_path: &Utf8Path) -> HarnessResult<Self> {
        let (parent_dir, file_name) = match open_rooted_parent(cassette_path, false) {
            Ok(result) => result,
            Err(HarnessError::Io { source }) if source.kind() == std::io::ErrorKind::NotFound => {
                return Err(HarnessError::CassetteNotFound {
                    cassette_name: cassette_name(cassette_path)?,
                });
            }
            Err(error) => return Err(error),
        };
        let file = match parent_dir.open(&file_name) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err(HarnessError::CassetteNotFound {
                    cassette_name: file_name.clone(),
                });
            }
            Err(error) => return Err(HarnessError::from(error)),
        };
        let cassette = Cassette::from_reader(file)?;
        Ok(Self {
            parent_dir,
            file_name,
            cassette,
        })
    }

    /// Replaces the in-memory cassette and persists it atomically.
    ///
    /// # Errors
    ///
    /// Returns a harness error when the updated cassette cannot be written.
    #[cfg(test)]
    pub(crate) fn save(&mut self, cassette: Cassette) -> HarnessResult<()> {
        let previous = self.cassette.clone();
        self.cassette = cassette;
        if let Err(error) = self.flush() {
            self.cassette = previous;
            return Err(error);
        }
        Ok(())
    }

    fn flush(&mut self) -> HarnessResult<()> {
        let temp_name = format!("{}.tmp", self.file_name);
        let mut temp_file = self.parent_dir.create(&temp_name)?;
        let write_result = self
            .cassette
            .write_to(&mut temp_file)
            .and_then(|()| temp_file.sync_all().map_err(HarnessError::from));
        if let Err(error) = write_result {
            drop(self.parent_dir.remove_file(&temp_name));
            return Err(error);
        }
        self.parent_dir
            .rename(&temp_name, &self.parent_dir, &self.file_name)?;
        Ok(())
    }
}

impl CassetteReader for FilesystemCassetteStore {
    fn load(&self) -> HarnessResult<Cassette> {
        Ok(self.cassette.clone())
    }
}

impl CassetteAppender for FilesystemCassetteStore {
    fn append(&mut self, interaction: Interaction) -> HarnessResult<()> {
        self.cassette.append(interaction.clone());
        if let Err(error) = self.flush() {
            self.cassette.interactions.pop();
            return Err(error);
        }
        Ok(())
    }
}

/// Verifies that record mode can create and remove sibling files.
///
/// This probes the same parent directory permissions used by append
/// persistence so startup can fail early on read-only targets.
pub(crate) fn probe_record_write_access(cassette_path: &Utf8Path) -> HarnessResult<()> {
    let (parent_dir, file_name) = open_rooted_parent(cassette_path, true)?;
    let probe_name = format!("{}.startup-probe-{}.tmp", file_name, probe_suffix());
    let probe_file = parent_dir.create(&probe_name);

    match probe_file {
        Ok(file) => {
            drop(file);
            parent_dir.remove_file(&probe_name)?;
            Ok(())
        }
        Err(error) => Err(HarnessError::from(error)),
    }
}

/// Returns `true` when `path` refers to the current directory and therefore
/// needs no explicit open or creation.
fn is_trivial_parent(path: &Utf8Path) -> bool {
    path.as_str().is_empty() || path == Utf8Path::new(".")
}

#[expect(
    clippy::collapsible_if,
    reason = "the nested structure keeps the trivial-parent rule explicit per review request"
)]
fn open_rooted_parent(
    cassette_path: &Utf8Path,
    create_parent: bool,
) -> HarnessResult<(Dir, String)> {
    let root = Dir::open_ambient_dir(".", ambient_authority())?;
    let file_name = cassette_name(cassette_path)?;
    let parent_option = cassette_path.parent();

    if create_parent {
        if let Some(parent_path) = parent_option {
            if !is_trivial_parent(parent_path) {
                root.create_dir_all(parent_path)?;
            }
        }
    }

    let parent_dir = match parent_option {
        None => root.try_clone()?,
        Some(parent_path) if is_trivial_parent(parent_path) => root.try_clone()?,
        Some(parent_path) => root.open_dir(parent_path)?,
    };

    Ok((parent_dir, file_name))
}

fn cassette_name(cassette_path: &Utf8Path) -> HarnessResult<String> {
    cassette_path
        .file_name()
        .map(ToOwned::to_owned)
        .ok_or_else(|| HarnessError::InvalidConfig {
            message: "cassette name must not be empty".to_owned(),
        })
}

fn probe_suffix() -> u128 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    use std::thread;
    use std::time::{SystemTime, UNIX_EPOCH};

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let pid = u128::from(std::process::id());
    let mut thread_hasher = DefaultHasher::new();
    thread::current().id().hash(&mut thread_hasher);
    let thread = u128::from(thread_hasher.finish());

    timestamp ^ (pid << 64) ^ thread
}

#[cfg(test)]
mod tests {
    //! Unit tests for the filesystem cassette adapter.

    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use camino::Utf8PathBuf;
    use rstest::rstest;
    use serde_json::json;
    use uuid::Uuid;

    use crate::HarnessError;
    use crate::cassette::{
        InteractionMetadata, RecordedRequest, RecordedResponse, StreamEvent, StreamTiming,
    };

    static NEXT_TEST_DIR: AtomicUsize = AtomicUsize::new(1);

    #[rstest]
    fn record_mode_creates_an_empty_versioned_cassette() {
        let cassette_path = unique_cassette_path("create");

        let store = FilesystemCassetteStore::open_or_create_for_record(&cassette_path)
            .expect("record mode should create a missing cassette");

        assert_eq!(
            store.load().expect("created cassette should load"),
            Cassette::new(),
        );
    }

    #[rstest]
    fn append_persists_interactions_in_order() {
        let cassette_path = unique_cassette_path("append");
        let mut store = FilesystemCassetteStore::open_or_create_for_record(&cassette_path)
            .expect("record mode should open cassette");
        let first = sample_interaction("first");
        let second = sample_interaction("second");

        CassetteAppender::append(&mut store, first.clone()).expect("first append should succeed");
        CassetteAppender::append(&mut store, second.clone()).expect("second append should succeed");

        let reloaded = FilesystemCassetteStore::open_for_replay(&cassette_path)
            .expect("replay load should succeed")
            .load()
            .expect("reloaded cassette should decode");

        assert_eq!(reloaded.interactions, vec![first, second]);
    }

    #[rstest]
    fn replay_load_rejects_unsupported_format_version() {
        let supported = crate::cassette::CassetteFormatVersion::SUPPORTED.as_u32();
        let cassette_path = unique_cassette_path("unsupported");
        let mut store = FilesystemCassetteStore::open_or_create_for_record(&cassette_path)
            .expect("record mode should create cassette");
        store.cassette.format_version = crate::cassette::CassetteFormatVersion::from(99);
        store
            .flush()
            .expect("writing unsupported cassette should succeed");

        let error = FilesystemCassetteStore::open_for_replay(&cassette_path)
            .expect_err("unsupported cassette version should fail");

        assert!(matches!(
            error,
            HarnessError::UnsupportedCassetteFormatVersion {
                found: 99,
                supported: found_supported,
            }
            if found_supported == supported
        ));
    }

    #[rstest]
    fn replay_load_maps_missing_files_to_cassette_not_found() {
        let cassette_path = unique_cassette_path("missing");
        let expected_name = cassette_path
            .file_name()
            .expect("generated cassette path should include a file name")
            .to_owned();

        let error = FilesystemCassetteStore::open_for_replay(&cassette_path)
            .expect_err("missing cassette should fail");

        assert!(matches!(
            error,
            HarnessError::CassetteNotFound { cassette_name }
                if cassette_name == expected_name
        ));
    }

    fn unique_cassette_path(name: &str) -> Utf8PathBuf {
        let index = NEXT_TEST_DIR.fetch_add(1, Ordering::Relaxed);
        let uuid = Uuid::new_v4();
        Utf8PathBuf::from(format!("target/test-cassettes/{name}-{index}-{uuid}.json"))
    }

    fn sample_interaction(content: &str) -> Interaction {
        Interaction {
            request: RecordedRequest {
                method: "POST".to_owned(),
                path: "/v1/chat/completions".to_owned(),
                query: String::new(),
                headers: Vec::new(),
                body: format!("request-{content}").into_bytes(),
                parsed_json: Some(json!({"content": content})),
                canonical_request: None,
                stable_hash: None,
            },
            response: RecordedResponse::Stream {
                status: 200,
                headers: Vec::new(),
                events: vec![
                    StreamEvent::Comment {
                        text: format!("comment-{content}"),
                    },
                    StreamEvent::Data {
                        raw: format!("data-{content}"),
                        parsed_json: Some(json!({"chunk": content})),
                    },
                ],
                raw_transcript: format!(": comment-{content}\n\ndata: data-{content}\n\n")
                    .into_bytes(),
                timing: Some(StreamTiming {
                    ttft_ms: 10,
                    chunk_offsets_ms: vec![10, 20],
                }),
            },
            metadata: InteractionMetadata {
                protocol_id: "openai.chat_completions.v1".to_owned(),
                upstream_id: "openrouter".to_owned(),
                recorded_at: "2026-03-10T00:00:00Z".to_owned(),
                relative_offset_ms: 0,
            },
        }
    }
}
