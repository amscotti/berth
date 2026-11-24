//! HTTP/1.1 over a Unix socket.
//!
//! The Docker Engine API is small enough that berth owns this transport instead
//! of pulling an async HTTP stack into the GPUI process. The rules that matter:
//! one total deadline per request (a peer that trickles bytes must not extend
//! it), chunked and content-length bodies, and a hard cap on what is buffered.
//!
//! One kind of request is read differently. `/events` is held open by the
//! engine until *its* clock reaches `until`, which can be a long way off when a
//! VM's clock has drifted. [`UnixClient::request_quiet`] samples such a stream:
//! it returns what has arrived instead of waiting for an end that may not come.

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
/// A prune walks whole stores too, and deletes as it goes.
pub const PRUNE_TIMEOUT: Duration = Duration::from_secs(120);
/// How long to sample a stream the engine may hold open, such as `/events`.
pub const EVENTS_WINDOW: Duration = Duration::from_millis(300);

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
        let deadline = Instant::now() + self.timeout;
        let mut stream = self.send(method, path, body)?;
        let (status, headers, leftover) = read_headers(&mut stream, deadline, self.timeout, false)?
            .ok_or(TransportError::Timeout(self.timeout))?;
        let mut reader = DeadlineReader::new(&mut stream, leftover, deadline, self.timeout);
        let body = read_body(&mut reader, status, &headers)?;
        Ok(Response {
            status,
            headers,
            body,
        })
    }

    /// For a stream berth only samples. Reading stops once `quiet_after` has
    /// passed since the request was sent, and whatever arrived is returned:
    /// a chunk cut off in the middle is dropped, whole ones are kept. `Ok(None)`
    /// means not even headers had arrived, which is not an error either. A
    /// malformed answer still fails.
    pub fn request_quiet(
        &self,
        method: &str,
        path: &str,
        body: &[u8],
        quiet_after: Duration,
    ) -> Result<Option<Response>, TransportError> {
        let started = Instant::now();
        let deadline = started + self.timeout;
        let mut stream = self.send(method, path, body)?;
        let quiet_by = (started + quiet_after).min(deadline);
        let Some((status, headers, leftover)) =
            read_headers(&mut stream, quiet_by, self.timeout, true)?
        else {
            return Ok(None);
        };
        let mut reader = DeadlineReader::new(&mut stream, leftover, deadline, self.timeout);
        reader.quiet_until = Some(quiet_by);
        let body = read_body(&mut reader, status, &headers)?;
        Ok(Some(Response {
            status,
            headers,
            body,
        }))
    }

    /// Connect and write the request. The caller reads the answer.
    fn send(
        &self,
        method: &str,
        path: &str,
        body: &[u8],
    ) -> Result<std::os::unix::net::UnixStream, TransportError> {
        let mut stream =
            std::os::unix::net::UnixStream::connect(&self.socket).map_err(|source| {
                TransportError::Connect {
                    path: self.socket.clone(),
                    source,
                }
            })?;
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
        Ok(stream)
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

/// Read the header block. `None`, only while sampling, when nothing had
/// arrived by the deadline.
fn read_headers(
    stream: &mut std::os::unix::net::UnixStream,
    deadline: Instant,
    timeout: Duration,
    sampling: bool,
) -> Result<Option<HeaderBlock>, TransportError> {
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
            return Ok(Some((status, headers, leftover)));
        }

        if sampling && Instant::now() >= deadline {
            return Ok(None);
        }
        let left = remaining(deadline, timeout)?;
        stream.set_read_timeout(Some(left)).ok();
        let n = match stream.read(&mut chunk) {
            Ok(n) => n,
            Err(error) => match map_read_error(error, timeout) {
                // A quiet peer is how a sample ends early; it isn't a failure.
                TransportError::Timeout(_) if sampling => return Ok(None),
                error => return Err(error),
            },
        };
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TransferEncoding {
    None,
    Chunked,
    Unsupported,
}

fn transfer_encoding(headers: &[(String, String)]) -> TransferEncoding {
    let mut tokens = Vec::new();
    for (key, value) in headers {
        if key == "transfer-encoding" {
            for part in value.split(',') {
                let token = part.trim().to_ascii_lowercase();
                if !token.is_empty() {
                    tokens.push(token);
                }
            }
        }
    }
    if tokens.is_empty() {
        TransferEncoding::None
    } else if tokens.len() == 1 && tokens[0] == "chunked" {
        TransferEncoding::Chunked
    } else if tokens.len() == 1 && tokens[0] == "identity" {
        TransferEncoding::None
    } else {
        TransferEncoding::Unsupported
    }
}

fn content_length(headers: &[(String, String)]) -> Option<usize> {
    headers
        .iter()
        .find(|(key, _)| key == "content-length")
        .and_then(|(_, value)| value.trim().parse::<usize>().ok())
}

fn read_body(
    reader: &mut DeadlineReader,
    status: u16,
    headers: &[(String, String)],
) -> Result<Vec<u8>, TransportError> {
    if status == 204 || status == 304 || (100..200).contains(&status) {
        return Ok(Vec::new());
    }
    match transfer_encoding(headers) {
        TransferEncoding::Chunked => return reader.read_chunked(),
        TransferEncoding::Unsupported => {
            return Err(TransportError::Malformed("unsupported transfer-encoding"));
        }
        TransferEncoding::None => {}
    }
    match content_length(headers) {
        Some(length) if length > MAX_BODY_BYTES => Err(TransportError::BodyTooLarge),
        Some(length) => reader.read_body_of(length),
        None => reader.read_to_end(),
    }
}

/// Reads the body after the header block, first draining bytes that arrived in
/// the same socket read as the headers.
struct DeadlineReader<'a> {
    stream: &'a mut std::os::unix::net::UnixStream,
    /// Bytes read from the socket and not yet consumed.
    buffer: Vec<u8>,
    pos: usize,
    deadline: Instant,
    timeout: Duration,
    /// Set while sampling: once this instant passes, a read that would wait
    /// returns nothing instead.
    quiet_until: Option<Instant>,
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
            quiet_until: None,
        }
    }

    fn sampling(&self) -> bool {
        self.quiet_until.is_some()
    }

    /// Read into `out`, from the leftover buffer first. Zero means the peer
    /// closed the connection or, while sampling, that it is time to stop.
    fn fill(&mut self, out: &mut [u8]) -> Result<usize, TransportError> {
        if self.pos < self.buffer.len() {
            let available = &self.buffer[self.pos..];
            let n = available.len().min(out.len());
            out[..n].copy_from_slice(&available[..n]);
            self.pos += n;
            return Ok(n);
        }
        if let Some(quiet_until) = self.quiet_until
            && Instant::now() >= quiet_until
        {
            return Ok(0);
        }
        // Never wait past the quiet point: a read that begins before it must
        // still end at it, or a held stream would block until the deadline.
        let mut left = remaining(self.deadline, self.timeout)?;
        if let Some(quiet_until) = self.quiet_until {
            left = left.min(quiet_until.saturating_duration_since(Instant::now()));
        }
        self.stream
            .set_read_timeout(Some(left.max(Duration::from_millis(1))))
            .ok();
        match self.stream.read(out) {
            Ok(n) => Ok(n),
            Err(error) => match map_read_error(error, self.timeout) {
                TransportError::Timeout(_) if self.sampling() => Ok(0),
                error => Err(error),
            },
        }
    }

    /// Pull more bytes into the leftover buffer. Zero means the stream ended.
    fn pull(&mut self) -> Result<usize, TransportError> {
        self.buffer.clear();
        self.pos = 0;
        let mut chunk = [0u8; 4096];
        let n = self.fill(&mut chunk)?;
        self.buffer.extend_from_slice(&chunk[..n]);
        Ok(n)
    }

    /// Read up to `out.len()` bytes, returning how many actually arrived.
    /// Fewer than `out.len()` means the stream ended first.
    fn read_exact_slice(&mut self, out: &mut [u8]) -> Result<usize, TransportError> {
        let mut read = 0;
        while read < out.len() {
            let n = self.fill(&mut out[read..])?;
            if n == 0 {
                return Ok(read);
            }
            read += n;
        }
        Ok(read)
    }

    /// A body of known length. A sample may be cut short (keeping only the
    /// bytes that arrived); a request may not.
    fn read_body_of(&mut self, len: usize) -> Result<Vec<u8>, TransportError> {
        let mut out = vec![0u8; len];
        let read = self.read_exact_slice(&mut out)?;
        if read == len {
            Ok(out)
        } else if self.sampling() {
            out.truncate(read);
            Ok(out)
        } else {
            Err(TransportError::Malformed("short body"))
        }
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

    /// One CRLF-terminated line, without its terminator. `None` when the
    /// stream ended first.
    fn read_line(&mut self) -> Result<Option<Vec<u8>>, TransportError> {
        let mut line = Vec::new();
        loop {
            while self.pos < self.buffer.len() {
                let byte = self.buffer[self.pos];
                self.pos += 1;
                line.push(byte);
                if line.ends_with(b"\r\n") {
                    line.truncate(line.len() - 2);
                    return Ok(Some(line));
                }
                if line.len() > 1024 {
                    return Err(TransportError::Malformed("chunk line too long"));
                }
            }
            if self.pull()? == 0 {
                return Ok(None);
            }
        }
    }

    fn read_chunked(&mut self) -> Result<Vec<u8>, TransportError> {
        let mut out = Vec::new();
        loop {
            let Some(line) = self.read_line()? else {
                return self.ended(out, "connection closed in chunk");
            };
            let size_text = String::from_utf8_lossy(&line);
            let size_hex = size_text.split(';').next().unwrap_or("").trim();
            let size = usize::from_str_radix(size_hex, 16)
                .map_err(|_| TransportError::Malformed("bad chunk size"))?;
            if size == 0 {
                // Trailers end at an empty line; a stream that ends first is
                // done too.
                while let Some(trailer) = self.read_line()? {
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
            let start = out.len();
            out.resize(start + size, 0);
            if self.read_exact_slice(&mut out[start..])? < size {
                out.truncate(start);
                return self.ended(out, "short body");
            }
            let mut terminator = [0u8; 2];
            if self.read_exact_slice(&mut terminator)? < 2 {
                return self.ended(out, "missing chunk terminator");
            }
            if &terminator != b"\r\n" {
                return Err(TransportError::Malformed("missing chunk terminator"));
            }
        }
        Ok(out)
    }

    /// The stream ended early. That is how a sample ends; for a request it is
    /// a malformed answer.
    fn ended(&self, out: Vec<u8>, how: &'static str) -> Result<Vec<u8>, TransportError> {
        if self.sampling() {
            Ok(out)
        } else {
            Err(TransportError::Malformed(how))
        }
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
mod tests;
