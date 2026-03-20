//! Shared BDD fixtures for integration tests.

use std::sync::atomic::{AtomicUsize, Ordering};

use uuid::Uuid;

static NEXT_TEST_CASSETTE: AtomicUsize = AtomicUsize::new(1);

pub(crate) fn unique_cassette_name(prefix: &str) -> String {
    let index = NEXT_TEST_CASSETTE.fetch_add(1, Ordering::Relaxed);
    let uuid = Uuid::new_v4();
    format!("{prefix}-{index}-{uuid}")
}
