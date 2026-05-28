//! Debug-format tests for adapter-neutral HTTP exchange types.

use futures_util::StreamExt;
use insta::assert_snapshot;
use rstest::rstest;

use super::{ProxyBody, ProxyResponse};

#[rstest]
fn proxy_response_debug_lists_header_names_without_values() {
    let response = ProxyResponse {
        status: 200,
        headers: vec![
            ("set-cookie".to_owned(), b"session=secret".to_vec()),
            ("authorization".to_owned(), b"Bearer secret".to_vec()),
        ],
        body: ProxyBody::Buffered(b"ok".to_vec()),
    };

    assert_snapshot!(format!("{response:?}"), @r###"ProxyResponse { status: 200, headers: ["set-cookie", "authorization"], body: Buffered(2) }"###);
}

#[rstest]
fn proxy_body_debug_summarizes_buffered_and_streaming_bodies() {
    let stream = futures_util::stream::empty().boxed();

    assert_snapshot!(format!("{:?}", ProxyBody::Buffered(b"secret".to_vec())), @"Buffered(6)");
    assert_snapshot!(format!("{:?}", ProxyBody::Stream(stream)), @"Stream");
}
