# Migration guide for 0.1.0

This guide covers breaking changes introduced by record-mode proxying.

## Header field type changes

Header values that travel through the proxy path now preserve raw bytes:

- `ObservedRequest.forward_headers` is `Vec<(String, Vec<u8>)>`.
- `ChatCompletionsRequest.headers` is `&[(String, Vec<u8>)]`.
- `ObservedResponse.proxy_headers` is `Vec<(String, Vec<u8>)>`.

Tests that previously asserted string header values should compare byte slices
on the proxy path. Cassette assertions should keep using strings because
non-UTF-8 values are percent-encoded before persistence.

## Redaction defaults

`RedactionConfig::default()` now drops `authorization`. To retain that header in
cassettes, configure an explicit empty rule set:

```rust
use spycatcher_harness::config::RedactionConfig;

let redaction = RedactionConfig {
    drop_headers: vec![],
};
```

Extend the default by including `authorization` plus any additional secret
headers:

```rust
use spycatcher_harness::config::RedactionConfig;

let redaction = RedactionConfig {
    drop_headers: vec![
        "authorization".to_owned(),
        "x-my-secret".to_owned(),
    ],
};
```

## Logging privacy

Record-mode request logging uses `uri.path()` only. Query strings are excluded
from structured request logs so tokens passed through query parameters are not
emitted by the harness.

## Test updates

Update proxy-path assertions to compare raw bytes:

```rust
assert_eq!(headers, vec![("x-raw".to_owned(), b"\xff\xfe".to_vec())]);
```

Update cassette assertions for non-UTF-8 header values to expect
percent-encoded strings:

```rust
assert_eq!(headers, vec![("x-raw".to_owned(), "%FF%FE".to_owned())]);
```
