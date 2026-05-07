//! BDD scenarios for record-mode chat completions proxying.

use rstest::fixture;
use rstest_bdd_macros::scenario;

mod record_mode_proxying;

use record_mode_proxying::world::ProxyWorld;

#[fixture]
fn proxy_world() -> ProxyWorld {
    ProxyWorld::default()
}

#[scenario(
    path = "tests/features/record_mode_proxying.feature",
    name = "Successful non-stream proxying records one interaction"
)]
fn successful_non_stream_proxying_records_one_interaction(proxy_world: ProxyWorld) {
    let _ = proxy_world;
}

#[scenario(
    path = "tests/features/record_mode_proxying.feature",
    name = "Redacted headers are not persisted"
)]
fn redacted_headers_are_not_persisted(proxy_world: ProxyWorld) {
    let _ = proxy_world;
}

#[scenario(
    path = "tests/features/record_mode_proxying.feature",
    name = "Authorization is redacted by default"
)]
fn authorization_is_redacted_by_default(proxy_world: ProxyWorld) {
    let _ = proxy_world;
}

#[scenario(
    path = "tests/features/record_mode_proxying.feature",
    name = "Streaming requests are rejected until streaming support lands"
)]
fn streaming_requests_are_rejected_until_streaming_support_lands(proxy_world: ProxyWorld) {
    let _ = proxy_world;
}

#[scenario(
    path = "tests/features/record_mode_proxying.feature",
    name = "Upstream transport failures do not write to the cassette"
)]
fn upstream_transport_failures_do_not_write_to_the_cassette(proxy_world: ProxyWorld) {
    let _ = proxy_world;
}
