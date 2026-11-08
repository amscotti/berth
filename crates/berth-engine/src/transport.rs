//! HTTP/1.1 over a Unix socket.
//!
//! The Docker Engine API is small enough that berth owns this transport instead
//! of pulling an async HTTP stack into the GPUI process. The rules that matter:
//! one total deadline per request (a peer that trickles bytes must not extend
//! it), chunked and content-length bodies, and a hard cap on what is buffered.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use thiserror::Error;

/// The largest response body berth will buffer.
pub const MAX_BODY_BYTES: usize = 8 * 1024 * 1024;
/// The largest response header block berth will buffer.
pub const MAX_HEADER_BYTES: usize = 64 * 1024;
/// One request's total receive/send budget.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);
/// A probe is cheaper than a list call.
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(3);
/// `/system/df` walks every store.
pub const DF_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Error)]
pub enum TransportError {
    #[error("could not connect to {path}: {source}")]
    Connect {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("request timed out after {0:?}")]
    Timeout(Duration),
    #[error("response headers exceeded {MAX_HEADER_BYTES} bytes")]
    HeadersTooLarge,
    #[error("response body exceeded {MAX_BODY_BYTES} bytes")]
    BodyTooLarge,
    #[error("malformed response: {0}")]
    Malformed(&'static str),
}

impl TransportError {
    /// True when the underlying Unix socket could not be reached or was dropped,
    /// as opposed to a slow response timeout or an oversized body.
    pub fn is_connection_lost(&self) -> bool {
        match self {
            Self::Connect { .. } => true,
            Self::Io(error) => matches!(
                error.kind(),
                std::io::ErrorKind::NotFound
                    | std::io::ErrorKind::ConnectionRefused
                    | std::io::ErrorKind::ConnectionReset
                    | std::io::ErrorKind::ConnectionAborted
                    | std::io::ErrorKind::BrokenPipe
                    | std::io::ErrorKind::PermissionDenied
            ),
            Self::Timeout(_) | Self::HeadersTooLarge | Self::BodyTooLarge | Self::Malformed(_) => {
                false
            }
        }
    }
}

/// A raw HTTP response.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Response {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    /// 2xx, or 304 for a lifecycle call that says "already there".
    pub fn ok_for(&self, allow_not_modified: bool) -> bool {
        (200..300).contains(&self.status) || (allow_not_modified && self.status == 304)
    }
}

/// A blocking Unix-socket HTTP client. Cheap to clone; it holds only a path.
#[derive(Clone, Debug)]
pub struct UnixClient {
    socket: PathBuf,
    timeout: Duration,
}

impl UnixClient {
    pub fn new(socket: impl Into<PathBuf>) -> Self {
        Self {
            socket: socket.into(),
            timeout: DEFAULT_TIMEOUT,
        }
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn socket(&self) -> &Path {
        &self.socket
    }

    /// One request, one response, connection closed. `path` is sent as given.
    pub fn request(
        &self,
        method: &str,
        path: &str,
        body: &[u8],
    ) -> Result<Response, TransportError> {
        let mut stream =
            std::os::unix::net::UnixStream::connect(&self.socket).map_err(|source| {
                TransportError::Connect {
                    path: self.socket.clone(),
                    source,
                }
            })?;
        let deadline = Instant::now() + self.timeout;
        stream.set_write_timeout(Some(self.timeout))?;
        stream.set_read_timeout(Some(self.timeout))?;

        let mut request = Vec::with_capacity(128 + body.len());
        write!(request, "{method} {path} HTTP/1.1\r\n")?;
        request.extend_from_slice(b"Host: docker\r\n");
        request.extend_from_slice(b"Connection: close\r\n");
        request.extend_from_slice(b"Accept: application/json\r\n");
        if !body.is_empty() {
            request.extend_from_slice(b"Content-Type: application/json\r\n");
            write!(request, "Content-Length: {}\r\n", body.len())?;
        }
        request.extend_from_slice(b"\r\n");
        request.extend_from_slice(body);

        stream.write_all(&request)?;
        stream.flush()?;

        let (status, headers, leftover) = read_headers(&mut stream, deadline, self.timeout)?;
        let chunked = headers.iter().any(|(key, value)| {
            key == "transfer-encoding" && value.to_ascii_lowercase().contains("chunked")
        });
        let content_length = headers
            .iter()
            .find(|(key, _)| key == "content-length")
            .and_then(|(_, value)| value.trim().parse::<usize>().ok());

        let mut reader = DeadlineReader::new(&mut stream, leftover, deadline, self.timeout);
        let body = if status == 204 || status == 304 || (100..200).contains(&status) {
            Vec::new()
        } else if chunked {
            reader.read_chunked()?
        } else if let Some(length) = content_length {
            if length > MAX_BODY_BYTES {
                return Err(TransportError::BodyTooLarge);
            }
            reader.read_exact_vec(length)?
        } else {
            reader.read_to_end()?
        };

        Ok(Response {
            status,
            headers,
            body,
        })
    }
}

fn remaining(deadline: Instant, timeout: Duration) -> Result<Duration, TransportError> {
    let now = Instant::now();
    if now >= deadline {
        return Err(TransportError::Timeout(timeout));
    }
    Ok((deadline - now).max(Duration::from_millis(1)))
}

type HeaderBlock = (u16, Vec<(String, String)>, Vec<u8>);

fn read_headers(
    stream: &mut std::os::unix::net::UnixStream,
    deadline: Instant,
    timeout: Duration,
) -> Result<HeaderBlock, TransportError> {
    let mut buffer = Vec::with_capacity(1024);
    let mut chunk = [0u8; 1024];

    loop {
        if let Some(end) = find_subslice(&buffer, b"\r\n\r\n") {
            let head = buffer[..end].to_vec();
            let leftover = buffer[end + 4..].to_vec();
            let text = String::from_utf8_lossy(&head);
            let mut lines = text.split("\r\n");
            let status_line = lines
                .next()
                .ok_or(TransportError::Malformed("missing status line"))?;
            let mut parts = status_line.split_whitespace();
            let _version = parts.next();
            let status = parts
                .next()
                .and_then(|value| value.parse::<u16>().ok())
                .ok_or(TransportError::Malformed("bad status line"))?;
            let mut headers = Vec::new();
            for line in lines {
                if line.is_empty() {
                    continue;
                }
                let (key, value) = line
                    .split_once(':')
                    .ok_or(TransportError::Malformed("bad header"))?;
                headers.push((key.trim().to_ascii_lowercase(), value.trim().to_owned()));
            }
            return Ok((status, headers, leftover));
        }

        let left = remaining(deadline, timeout)?;
        stream.set_read_timeout(Some(left)).ok();
        let n = stream
            .read(&mut chunk)
            .map_err(|error| map_read_error(error, timeout))?;
        if n == 0 {
            return Err(TransportError::Io(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "connection closed in headers",
            )));
        }
        buffer.extend_from_slice(&chunk[..n]);
        if buffer.len() > MAX_HEADER_BYTES {
            return Err(TransportError::HeadersTooLarge);
        }
    }
}

/// Reads the body after the header block, first draining bytes that arrived in
/// the same socket read as the headers.
struct DeadlineReader<'a> {
    stream: &'a mut std::os::unix::net::UnixStream,
    buffer: Vec<u8>,
    pos: usize,
    deadline: Instant,
    timeout: Duration,
}

impl<'a> DeadlineReader<'a> {
    fn new(
        stream: &'a mut std::os::unix::net::UnixStream,
        buffer: Vec<u8>,
        deadline: Instant,
        timeout: Duration,
    ) -> Self {
        Self {
            stream,
            buffer,
            pos: 0,
            deadline,
            timeout,
        }
    }

    fn fill(&mut self, out: &mut [u8]) -> Result<usize, TransportError> {
        if self.pos < self.buffer.len() {
            let available = &self.buffer[self.pos..];
            let n = available.len().min(out.len());
            out[..n].copy_from_slice(&available[..n]);
            self.pos += n;
            return Ok(n);
        }
        let left = remaining(self.deadline, self.timeout)?;
        self.stream.set_read_timeout(Some(left)).ok();
        self.stream
            .read(out)
            .map_err(|error| map_read_error(error, self.timeout))
    }

    fn read_exact_slice(&mut self, out: &mut [u8]) -> Result<(), TransportError> {
        let mut read = 0;
        while read < out.len() {
            let n = self.fill(&mut out[read..])?;
            if n == 0 {
                return Err(TransportError::Malformed("short body"));
            }
            read += n;
        }
        Ok(())
    }

    fn read_exact_vec(&mut self, len: usize) -> Result<Vec<u8>, TransportError> {
        let mut out = vec![0u8; len];
        self.read_exact_slice(&mut out)?;
        Ok(out)
    }

    fn read_exact_into(&mut self, out: &mut Vec<u8>, len: usize) -> Result<(), TransportError> {
        let start = out.len();
        out.resize(start + len, 0);
        self.read_exact_slice(&mut out[start..])
    }

    fn read_to_end(&mut self) -> Result<Vec<u8>, TransportError> {
        let mut out = Vec::new();
        let mut chunk = [0u8; 8192];
        loop {
            let n = self.fill(&mut chunk)?;
            if n == 0 {
                break;
            }
            out.extend_from_slice(&chunk[..n]);
            if out.len() > MAX_BODY_BYTES {
                return Err(TransportError::BodyTooLarge);
            }
        }
        Ok(out)
    }

    fn read_line(&mut self) -> Result<Vec<u8>, TransportError> {
        let mut line = Vec::new();
        let mut chunk = [0u8; 4096];
        loop {
            while self.pos < self.buffer.len() {
                let b = self.buffer[self.pos];
                self.pos += 1;
                line.push(b);
                if line.ends_with(b"\r\n") {
                    line.truncate(line.len() - 2);
                    return Ok(line);
                }
                if line.len() > 1024 {
                    return Err(TransportError::Malformed("chunk line too long"));
                }
            }
            self.buffer.clear();
            self.pos = 0;
            let left = remaining(self.deadline, self.timeout)?;
            self.stream.set_read_timeout(Some(left)).ok();
            let n = self
                .stream
                .read(&mut chunk)
                .map_err(|error| map_read_error(error, self.timeout))?;
            if n == 0 {
                return Err(TransportError::Malformed("connection closed in chunk"));
            }
            self.buffer.extend_from_slice(&chunk[..n]);
        }
    }

    fn read_chunked(&mut self) -> Result<Vec<u8>, TransportError> {
        let mut out = Vec::new();
        loop {
            let line = self.read_line()?;
            let size_text = String::from_utf8_lossy(&line);
            let size_hex = size_text.split(';').next().unwrap_or("").trim();
            let size = usize::from_str_radix(size_hex, 16)
                .map_err(|_| TransportError::Malformed("bad chunk size"))?;
            if size == 0 {
                loop {
                    let trailer = self.read_line()?;
                    if trailer.is_empty() {
                        break;
                    }
                }
                break;
            }
            if out
                .len()
                .checked_add(size)
                .is_none_or(|total| total > MAX_BODY_BYTES)
            {
                return Err(TransportError::BodyTooLarge);
            }
            self.read_exact_into(&mut out, size)?;
            let mut terminator = [0u8; 2];
            self.read_exact_slice(&mut terminator)?;
            if &terminator != b"\r\n" {
                return Err(TransportError::Malformed("missing chunk terminator"));
            }
        }
        Ok(out)
    }
}

fn map_read_error(error: std::io::Error, timeout: Duration) -> TransportError {
    if matches!(
        error.kind(),
        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
    ) {
        TransportError::Timeout(timeout)
    } else {
        TransportError::Io(error)
    }
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(0);

    fn socket_path() -> PathBuf {
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("berth-transport-{}-{id}.sock", std::process::id()))
    }

    fn serve(
        response: impl FnOnce(&mut std::os::unix::net::UnixStream) + Send + 'static,
    ) -> PathBuf {
        let path = socket_path();
        let listener = std::os::unix::net::UnixListener::bind(&path).expect("bind");
        std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut request = [0u8; 4096];
                let _ = stream.read(&mut request);
                response(&mut stream);
            }
        });
        path
    }

    fn serve_bytes(response: &'static [u8]) -> PathBuf {
        serve(move |stream| {
            let _ = stream.write_all(response);
        })
    }

    #[test]
    fn reads_a_content_length_response() {
        let path = serve_bytes(
            b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nContent-Type: text/plain\r\n\r\nhello",
        );
        let response = UnixClient::new(path)
            .request("GET", "/version", b"")
            .expect("request");
        assert_eq!(response.status, 200);
        assert_eq!(response.body, b"hello");
        assert_eq!(response.header("content-type"), Some("text/plain"));
        assert!(response.ok_for(false));
    }

    #[test]
    fn decodes_a_chunked_response() {
        let path = serve_bytes(
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n6\r\n world\r\n0\r\n\r\n",
        );
        let response = UnixClient::new(path)
            .request("GET", "/", b"")
            .expect("request");
        assert_eq!(response.body, b"hello world");
    }

    #[test]
    fn reads_to_eof_without_a_length() {
        let path = serve_bytes(b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\nabc");
        let response = UnixClient::new(path)
            .request("GET", "/", b"")
            .expect("request");
        assert_eq!(response.body, b"abc");
    }

    #[test]
    fn a_304_is_ok_only_for_lifecycle_calls() {
        let path = serve_bytes(b"HTTP/1.1 304 Not Modified\r\nContent-Length: 0\r\n\r\n");
        let response = UnixClient::new(path)
            .request("POST", "/start", b"")
            .expect("request");
        assert!(!response.ok_for(false));
        assert!(response.ok_for(true));
    }

    #[test]
    fn a_trickle_cannot_extend_the_total_deadline() {
        let path = serve(|stream| {
            let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\n\r\n");
            for _ in 0..10 {
                std::thread::sleep(Duration::from_millis(80));
                let _ = stream.write_all(b"x");
            }
        });
        let started = Instant::now();
        let error = UnixClient::new(path)
            .with_timeout(Duration::from_millis(300))
            .request("GET", "/", b"")
            .expect_err("must time out");
        assert!(matches!(error, TransportError::Timeout(_)));
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn a_malformed_header_is_named() {
        let path = serve_bytes(b"not http\r\n\r\n");
        let error = UnixClient::new(path)
            .request("GET", "/", b"")
            .expect_err("must fail");
        assert!(matches!(error, TransportError::Malformed(_)));
    }

    #[test]
    fn an_oversized_body_is_refused_before_buffering_it() {
        let path = serve_bytes(b"HTTP/1.1 200 OK\r\nContent-Length: 999999999\r\n\r\n");
        let error = UnixClient::new(path)
            .request("GET", "/", b"")
            .expect_err("must fail");
        assert!(matches!(error, TransportError::BodyTooLarge));
    }
}
