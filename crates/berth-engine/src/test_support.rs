//! A scripted Unix-socket engine for tests.
//!
//! `docs/TESTING.md` §4 describes it: answer by method and path from a queue,
//! serve chunked, trickled, delayed, truncated and hanging responses, and
//! record every request so a test can assert on what was sent. Every path it
//! creates is removed when it is dropped, so a test run leaves nothing in the
//! temp directory.
//!
//! It is compiled for this crate's own tests, or for a crate that enables
//! `test-support` in its dev-dependencies. It is never part of a release.

use std::collections::VecDeque;
use std::io::{Read, Write};
use std::net::Shutdown;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// How often a waiting mock thread checks whether the mock was told to stop.
const POLL: Duration = Duration::from_millis(10);
/// How long `wait_for` polls a condition before giving up.
const WAIT_LIMIT: Duration = Duration::from_secs(2);

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

/// A directory under the system temp dir, removed with its contents on drop.
#[derive(Debug)]
pub struct TempDir(PathBuf);

impl TempDir {
    pub fn new(label: &str) -> Self {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("berth-{label}-{}-{id}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("create the temp dir");
        Self(path)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// One answer the mock can give.
#[derive(Clone, Debug)]
pub enum Reply {
    /// A status with a JSON body.
    Json(u16, String),
    /// Raw body bytes with a content length: Docker's log frames, for instance.
    Body(Vec<u8>),
    /// A chunked body, one chunk per piece.
    Chunked(Vec<Vec<u8>>),
    /// Literal wire bytes, sent exactly as written.
    Raw(Vec<u8>),
    /// Wait, then answer. For slow routes and deadline tests.
    After(Duration, Box<Reply>),
    /// Send each piece after a gap. For trickle tests.
    Trickle { gap: Duration, pieces: Vec<Vec<u8>> },
    /// Never answer; hold the connection until the mock stops or the client
    /// goes away.
    Hang,
    /// Headers that promise `promised` bytes, then `sent` bytes, then close.
    Truncated { promised: usize, sent: usize },
}

impl Reply {
    pub fn ok(body: impl Into<String>) -> Self {
        Reply::Json(200, body.into())
    }

    /// The shape the engine uses for its errors.
    pub fn error(status: u16, message: impl Into<String>) -> Self {
        let message = message.into();
        Reply::Json(status, format!(r#"{{"message":"{message}"}}"#))
    }

    /// One stdout frame in Docker's multiplexed log format.
    pub fn framed(text: &str) -> Self {
        let mut out = vec![1, 0, 0, 0];
        out.extend_from_slice(&(text.len() as u32).to_be_bytes());
        out.extend_from_slice(text.as_bytes());
        Reply::Body(out)
    }
}

/// A request the mock has answered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    pub method: String,
    pub path: String,
    pub body: Vec<u8>,
}

impl Request {
    /// `GET /v1.43/version`, for assertions and messages.
    pub fn line(&self) -> String {
        format!("{} {}", self.method, self.path)
    }
}

struct Scripted {
    method: Option<String>,
    path: String,
    replies: VecDeque<Reply>,
}

struct Shared {
    routes: Mutex<Vec<Scripted>>,
    requests: Mutex<Vec<Request>>,
    stopped: AtomicBool,
}

/// A mock engine listening on one Unix socket under the temp dir.
///
/// Routes match in the order they were added: the first whose path is
/// contained in the request path (and whose method, if given, matches) wins.
/// A route answers from its queue and repeats its last answer when the queue
/// runs out.
pub struct MockEngine {
    shared: Arc<Shared>,
    socket: PathBuf,
}

impl MockEngine {
    pub fn start() -> Self {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let socket =
            std::env::temp_dir().join(format!("berth-mock-{}-{id}.sock", std::process::id()));
        let _ = std::fs::remove_file(&socket);
        let listener = UnixListener::bind(&socket).expect("bind the mock socket");
        let shared = Arc::new(Shared {
            routes: Mutex::new(Vec::new()),
            requests: Mutex::new(Vec::new()),
            stopped: AtomicBool::new(false),
        });
        let server = Arc::clone(&shared);
        std::thread::spawn(move || serve(listener, server));
        Self { shared, socket }
    }

    /// Script a route. `spec` is `"GET /v1.43/version"`, `"/containers/json"`
    /// for any method, or `"*"` for everything no earlier route matched.
    pub fn on(&self, spec: &str) -> Route<'_> {
        let (method, path) = match spec.split_once(' ') {
            Some((method, path)) => (Some(method.to_ascii_uppercase()), path.to_owned()),
            None => (None, spec.to_owned()),
        };
        let mut routes = lock(&self.shared.routes);
        routes.push(Scripted {
            method,
            path,
            replies: VecDeque::new(),
        });
        Route {
            engine: self,
            index: routes.len() - 1,
        }
    }

    /// The socket path, as a client wants it.
    pub fn socket(&self) -> String {
        self.socket.to_string_lossy().into_owned()
    }

    /// Every request answered so far, in order.
    pub fn requests(&self) -> Vec<Request> {
        lock(&self.shared.requests).clone()
    }

    /// `METHOD path` for every request so far.
    pub fn paths(&self) -> Vec<String> {
        self.requests().iter().map(Request::line).collect()
    }

    /// How many requests so far contain `needle` in their path.
    pub fn count(&self, needle: &str) -> usize {
        self.paths()
            .iter()
            .filter(|line| line.contains(needle))
            .count()
    }

    /// Block until `n` requests containing `needle` have arrived, or two
    /// seconds pass. A wait on a condition: no test sleeps a fixed delay.
    pub fn wait_for(&self, needle: &str, n: usize) -> bool {
        let deadline = Instant::now() + WAIT_LIMIT;
        while self.count(needle) < n {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return false;
            }
            std::thread::sleep(POLL.min(left));
        }
        true
    }

    /// Stop answering and remove the socket. Safe to call more than once.
    pub fn stop(&self) {
        if self.shared.stopped.swap(true, Ordering::SeqCst) {
            return;
        }
        // Wake the accept loop while the socket still exists, then remove it.
        let _ = UnixStream::connect(&self.socket);
        let _ = std::fs::remove_file(&self.socket);
    }
}

impl Drop for MockEngine {
    fn drop(&mut self) {
        self.stop();
    }
}

/// A route under construction. `MockEngine::on` returns one. The route is
/// registered as soon as `on` returns, so a chain may end anywhere.
pub struct Route<'a> {
    engine: &'a MockEngine,
    index: usize,
}

impl<'a> Route<'a> {
    fn push(self, reply: Reply) -> Self {
        lock(&self.engine.shared.routes)[self.index]
            .replies
            .push_back(reply);
        self
    }

    /// Queue one more answer for this route.
    pub fn reply(self, reply: Reply) -> Self {
        self.push(reply)
    }

    /// Queue a JSON answer.
    pub fn json(self, status: u16, body: impl Into<String>) -> Self {
        self.push(Reply::Json(status, body.into()))
    }

    /// Queue a `200` JSON answer.
    pub fn ok(self, body: impl Into<String>) -> Self {
        self.push(Reply::ok(body))
    }

    /// Queue an engine error.
    pub fn error(self, status: u16, message: impl Into<String>) -> Self {
        self.push(Reply::error(status, message))
    }

    /// Script the next route.
    pub fn on(self, spec: &str) -> Route<'a> {
        self.engine.on(spec)
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn serve(listener: UnixListener, shared: Arc<Shared>) {
    for stream in listener.incoming() {
        if shared.stopped.load(Ordering::SeqCst) {
            break;
        }
        if let Ok(mut stream) = stream {
            let shared = Arc::clone(&shared);
            std::thread::spawn(move || handle(&mut stream, &shared));
        }
    }
}

fn handle(stream: &mut UnixStream, shared: &Shared) {
    let Some(request) = read_request(stream) else {
        return;
    };
    let reply = answer(shared, &request);
    write_reply(stream, &reply, &shared.stopped);
}

fn read_request(stream: &mut UnixStream) -> Option<Request> {
    let mut bytes = Vec::new();
    let mut chunk = [0u8; 4096];
    let head_end = loop {
        if let Some(at) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
            break at;
        }
        let n = stream.read(&mut chunk).ok()?;
        if n == 0 {
            return None;
        }
        bytes.extend_from_slice(&chunk[..n]);
        if bytes.len() > 64 * 1024 {
            return None;
        }
    };

    let head = String::from_utf8_lossy(&bytes[..head_end]).into_owned();
    let mut lines = head.split("\r\n");
    let mut first = lines.next()?.split_whitespace();
    let method = first.next()?.to_owned();
    let path = first.next()?.to_owned();
    let mut length = 0usize;
    for line in lines {
        if let Some((key, value)) = line.split_once(':')
            && key.eq_ignore_ascii_case("content-length")
        {
            length = value.trim().parse().ok()?;
        }
    }

    let mut body = bytes[head_end + 4..].to_vec();
    while body.len() < length {
        let n = stream.read(&mut chunk).ok()?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..n]);
    }
    body.truncate(length);
    Some(Request { method, path, body })
}

fn answer(shared: &Shared, request: &Request) -> Reply {
    lock(&shared.requests).push(request.clone());
    let mut routes = lock(&shared.routes);
    let Some(route) = routes.iter_mut().find(|route| {
        route.path == "*"
            || (request.path.contains(&route.path)
                && route
                    .method
                    .as_deref()
                    .is_none_or(|method| method == request.method))
    }) else {
        return Reply::error(404, format!("no route for {}", request.path));
    };
    let path = route.path.clone();
    if route.replies.len() > 1 {
        route.replies.pop_front()
    } else {
        route.replies.front().cloned()
    }
    .unwrap_or_else(|| Reply::error(500, format!("route {path} has no answer")))
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        201 => "Created",
        204 => "No Content",
        304 => "Not Modified",
        404 => "Not Found",
        500 => "Internal Server Error",
        _ => "",
    }
}

fn head(status: u16, length: Option<usize>) -> String {
    let mut head = format!(
        "HTTP/1.1 {status} {}\r\nConnection: close\r\n",
        reason(status)
    );
    match length {
        Some(length) => {
            head.push_str(&format!("Content-Length: {length}\r\n"));
        }
        None => head.push_str("Transfer-Encoding: chunked\r\n"),
    }
    head.push_str("\r\n");
    head
}

fn write_reply(stream: &mut UnixStream, reply: &Reply, stopped: &AtomicBool) {
    match reply {
        Reply::Json(status, body) => {
            let _ = stream.write_all(head(*status, Some(body.len())).as_bytes());
            let _ = stream.write_all(body.as_bytes());
        }
        Reply::Body(body) => {
            let _ = stream.write_all(head(200, Some(body.len())).as_bytes());
            let _ = stream.write_all(body);
        }
        Reply::Chunked(pieces) => {
            let _ = stream.write_all(head(200, None).as_bytes());
            for piece in pieces {
                if write_all(stream, format!("{:x}\r\n", piece.len()).as_bytes(), stopped)
                    && write_all(stream, piece, stopped)
                    && write_all(stream, b"\r\n", stopped)
                {
                    continue;
                }
                return;
            }
            let _ = write_all(stream, b"0\r\n\r\n", stopped);
        }
        Reply::Raw(bytes) => {
            let _ = write_all(stream, bytes, stopped);
        }
        Reply::After(delay, reply) => {
            if pause(*delay, stopped) {
                write_reply(stream, reply, stopped);
            }
        }
        Reply::Trickle { gap, pieces } => {
            for piece in pieces {
                if !pause(*gap, stopped) || !write_all(stream, piece, stopped) {
                    return;
                }
            }
        }
        Reply::Hang => hold(stream, stopped),
        Reply::Truncated { promised, sent } => {
            let _ = stream.write_all(head(200, Some(*promised)).as_bytes());
            let _ = stream.write_all(&vec![0u8; *sent]);
        }
    }
    let _ = stream.flush();
    let _ = stream.shutdown(Shutdown::Write);
}

fn write_all(stream: &mut UnixStream, bytes: &[u8], stopped: &AtomicBool) -> bool {
    if stopped.load(Ordering::SeqCst) {
        return false;
    }
    stream.write_all(bytes).is_ok()
}

/// Wait `delay`, giving up early once the mock stops. False when it stopped.
fn pause(delay: Duration, stopped: &AtomicBool) -> bool {
    let until = Instant::now() + delay;
    while let Some(left) = until.checked_duration_since(Instant::now()) {
        if stopped.load(Ordering::SeqCst) {
            return false;
        }
        std::thread::sleep(POLL.min(left));
    }
    !stopped.load(Ordering::SeqCst)
}

/// Hold a connection open until the mock stops or the client goes away.
fn hold(stream: &mut UnixStream, stopped: &AtomicBool) {
    let mut scratch = [0u8; 64];
    let _ = stream.set_read_timeout(Some(POLL));
    while !stopped.load(Ordering::SeqCst) {
        // A zero means the client closed its end, which is how a deadline
        // shows up here.
        if matches!(stream.read(&mut scratch), Ok(0)) {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_temp_dir_removes_itself_on_drop() {
        let path = {
            let dir = TempDir::new("guard");
            std::fs::write(dir.path().join("file"), b"x").expect("write");
            dir.path().to_path_buf()
        };
        assert!(!path.exists());
    }

    #[test]
    fn a_mock_answers_by_path_and_removes_its_socket_on_drop() {
        let socket = {
            let engine = MockEngine::start();
            engine.on("GET /version").ok(r#"{"Version":"1"}"#);
            let socket = engine.socket();
            let client = crate::client::Client::new(&socket);
            assert_eq!(client.version().expect("version").version, "1");
            assert_eq!(engine.paths(), ["GET /version"]);
            socket
        };
        assert!(!Path::new(&socket).exists());
    }

    #[test]
    fn a_route_repeats_its_last_answer_and_unrouted_paths_are_404s() {
        let engine = MockEngine::start();
        engine
            .on("GET /version")
            .error(500, "starting")
            .ok(r#"{"Version":"1"}"#);
        let client = crate::client::Client::new(engine.socket());
        assert!(client.version().is_err());
        assert!(client.version().is_ok());
        assert!(client.version().is_ok());
        assert_eq!(engine.count("/version"), 3);
        assert!(client.info().is_err());
        assert_eq!(engine.count("/info"), 1);
    }
}
