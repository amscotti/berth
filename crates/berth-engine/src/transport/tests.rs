use super::*;
use crate::test_support::{MockEngine, Reply};

/// A mock that answers every request with these literal wire bytes.
fn serve(bytes: &[u8]) -> MockEngine {
    let engine = MockEngine::start();
    engine.on("*").reply(Reply::Raw(bytes.to_vec()));
    engine
}

/// A mock that sends these literal wire bytes, then holds the connection.
fn serve_then_hang(bytes: &[u8]) -> MockEngine {
    let engine = MockEngine::start();
    engine
        .on("*")
        .reply(Reply::Then(vec![Reply::Raw(bytes.to_vec()), Reply::Hang]));
    engine
}

/// Read a sample with a generous deadline, so the test fails if the read
/// waited for the deadline instead of stopping at its quiet point.
fn sample(engine: &MockEngine) -> Option<Response> {
    UnixClient::new(engine.socket())
        .with_timeout(Duration::from_secs(5))
        .request_quiet("GET", "/events", b"", Duration::from_millis(150))
        .expect("a quiet read is never a timeout")
}

#[test]
fn reads_a_content_length_response() {
    let engine =
        serve(b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nContent-Type: text/plain\r\n\r\nhello");
    let response = UnixClient::new(engine.socket())
        .request("GET", "/version", b"")
        .expect("request");
    assert_eq!(response.status, 200);
    assert_eq!(response.body, b"hello");
    assert_eq!(response.header("content-type"), Some("text/plain"));
    assert!(response.ok_for(false));
    assert_eq!(engine.paths(), ["GET /version"]);
}

#[test]
fn decodes_a_chunked_response() {
    let engine = MockEngine::start();
    engine
        .on("*")
        .reply(Reply::Chunked(vec![b"hello".to_vec(), b" world".to_vec()]));
    let response = UnixClient::new(engine.socket())
        .request("GET", "/", b"")
        .expect("request");
    assert_eq!(response.body, b"hello world");
}

#[test]
fn reads_to_eof_without_a_length() {
    let engine = serve(b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\nabc");
    let response = UnixClient::new(engine.socket())
        .request("GET", "/", b"")
        .expect("request");
    assert_eq!(response.body, b"abc");
}

#[test]
fn a_304_is_ok_only_for_lifecycle_calls() {
    let engine = serve(b"HTTP/1.1 304 Not Modified\r\nContent-Length: 0\r\n\r\n");
    let response = UnixClient::new(engine.socket())
        .request("POST", "/start", b"")
        .expect("request");
    assert!(!response.ok_for(false));
    assert!(response.ok_for(true));
}

#[test]
fn a_trickle_cannot_extend_the_total_deadline() {
    let mut pieces = vec![b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\n\r\n".to_vec()];
    pieces.extend((0..10).map(|_| b"x".to_vec()));
    let engine = MockEngine::start();
    engine.on("*").reply(Reply::Trickle {
        gap: Duration::from_millis(80),
        pieces,
    });
    let started = Instant::now();
    let error = UnixClient::new(engine.socket())
        .with_timeout(Duration::from_millis(300))
        .request("GET", "/", b"")
        .expect_err("must time out");
    assert!(matches!(error, TransportError::Timeout(_)));
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[test]
fn a_malformed_header_is_named() {
    let engine = serve(b"not http\r\n\r\n");
    let error = UnixClient::new(engine.socket())
        .request("GET", "/", b"")
        .expect_err("must fail");
    assert!(matches!(error, TransportError::Malformed(_)));
}

#[test]
fn an_oversized_body_is_refused_before_buffering_it() {
    let engine = serve(b"HTTP/1.1 200 OK\r\nContent-Length: 999999999\r\n\r\n");
    let error = UnixClient::new(engine.socket())
        .request("GET", "/", b"")
        .expect_err("must fail");
    assert!(matches!(error, TransportError::BodyTooLarge));
}

#[test]
fn a_sample_returns_what_arrived_without_waiting_for_the_deadline() {
    let engine = serve_then_hang(
        b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n8\r\n{\"a\":1}\n\r\n",
    );
    let started = Instant::now();
    let response = sample(&engine).expect("the headers arrived");
    assert_eq!(response.body, b"{\"a\":1}\n");
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[test]
fn a_sample_of_a_silent_peer_is_none_not_an_error() {
    let engine = MockEngine::start();
    engine.on("*").reply(Reply::Hang);
    let started = Instant::now();
    assert!(sample(&engine).is_none());
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[test]
fn a_sample_drops_the_chunk_it_was_cut_off_inside() {
    let engine = serve_then_hang(
        b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nabc\r\n10\r\npar",
    );
    let response = sample(&engine).expect("the headers arrived");
    assert_eq!(response.body, b"abc");
}

#[test]
fn a_request_still_refuses_a_truncated_chunk() {
    let engine = serve_then_hang(
        b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nabc\r\n10\r\npar",
    );
    UnixClient::new(engine.socket())
        .with_timeout(Duration::from_millis(300))
        .request("GET", "/events", b"")
        .expect_err("a request does not accept a cut-off body");
}

#[test]
fn a_sample_that_completely_arrived_ends_at_once() {
    // A stream the peer closes ends at once; the quiet point, set far out
    // here, is only for streams that stay open.
    let engine = MockEngine::start();
    engine
        .on("*")
        .reply(Reply::Chunked(vec![b"one".to_vec(), b"two".to_vec()]));
    let started = Instant::now();
    let response = UnixClient::new(engine.socket())
        .with_timeout(Duration::from_secs(5))
        .request_quiet("GET", "/events", b"", Duration::from_millis(3000))
        .expect("no error")
        .expect("the headers arrived");
    assert_eq!(response.body, b"onetwo");
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[test]
fn a_truncated_content_length_response_fails() {
    let engine = MockEngine::start();
    engine.on("*").reply(Reply::Truncated {
        promised: 100,
        sent: 12,
    });
    let error = UnixClient::new(engine.socket())
        .request("GET", "/", b"")
        .expect_err("short content-length body must fail");
    assert!(matches!(error, TransportError::Malformed("short body")));
}

#[test]
fn a_sampled_content_length_body_truncates_to_bytes_read() {
    let engine = serve_then_hang(b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\nhello");
    let response = sample(&engine).expect("headers arrived");
    assert_eq!(response.body, b"hello");
}

#[test]
fn an_unsupported_transfer_encoding_is_refused() {
    let engine = serve(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: gzip\r\n\r\nabc");
    let error = UnixClient::new(engine.socket())
        .request("GET", "/", b"")
        .expect_err("unsupported transfer-encoding must fail");
    assert!(matches!(
        error,
        TransportError::Malformed("unsupported transfer-encoding")
    ));
}
