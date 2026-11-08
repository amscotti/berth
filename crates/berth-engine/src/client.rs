//! The Docker Engine API as berth uses it. One blocking client, one request per
//! call, connection closed. The UI never calls this on its own thread.

use std::time::Duration;

use serde::de::DeserializeOwned;
use thiserror::Error;

use crate::detail::ContainerDetail;
use crate::df::Df;
use crate::discovery::Dialect;
use crate::dto::{
    Container, ContainerTop, FsChange, ImageLayer, Info, Network, Stats, Version, Volume,
};
use crate::events::{Event, parse_events};
use crate::guards;
use crate::transport::{DEFAULT_TIMEOUT, DF_TIMEOUT, Response, TransportError, UnixClient};

#[derive(Debug, Error)]
pub enum ClientError {
    #[error(transparent)]
    Transport(#[from] TransportError),
    #[error("engine returned {status}: {message}")]
    Engine { status: u16, message: String },
    #[error("could not parse the engine's answer: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Availability(String),
}

impl ClientError {
    pub fn message(&self) -> String {
        self.to_string()
    }
}

/// A client bound to one socket and one API version.
#[derive(Clone, Debug)]
pub struct Client {
    socket: String,
    api: String,
    timeout: Duration,
    dialect: Dialect,
}

impl Client {
    pub fn new(socket: impl Into<String>) -> Self {
        Self {
            socket: socket.into(),
            api: crate::DEFAULT_API_VERSION.to_owned(),
            timeout: DEFAULT_TIMEOUT,
            dialect: Dialect::Docker,
        }
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn with_api(mut self, api: impl Into<String>) -> Self {
        self.api = api.into();
        self
    }

    pub fn with_dialect(mut self, dialect: Dialect) -> Self {
        self.dialect = dialect;
        self
    }

    pub fn socket(&self) -> &str {
        &self.socket
    }
    pub fn api(&self) -> &str {
        &self.api
    }
    pub fn dialect(&self) -> Dialect {
        self.dialect
    }

    fn prefixed(&self, path: &str) -> String {
        format!("/{}{}", self.api, path)
    }

    fn call(
        &self,
        method: &str,
        path: &str,
        body: &[u8],
        raw: bool,
        timeout: Duration,
    ) -> Result<Response, ClientError> {
        let full = if raw {
            path.to_owned()
        } else {
            self.prefixed(path)
        };
        let response = UnixClient::new(self.socket.as_str())
            .with_timeout(timeout)
            .request(method, &full, body)?;
        let allow_not_modified = method != "GET";
        if !response.ok_for(allow_not_modified) {
            let message = engine_message(&response);
            return Err(ClientError::Engine {
                status: response.status,
                message,
            });
        }
        Ok(response)
    }

    fn parse<T: DeserializeOwned>(&self, body: &[u8]) -> Result<T, ClientError> {
        if guards::nesting_exceeds(body, guards::MAX_DEPTH) {
            return Err(ClientError::Availability(format!(
                "refusing to parse (availability limit — nesting over {})",
                guards::MAX_DEPTH
            )));
        }
        Ok(serde_json::from_slice(body)?)
    }

    fn parse_list<T: DeserializeOwned>(&self, body: &[u8]) -> Result<Vec<T>, ClientError> {
        if guards::array_len_exceeds(body, guards::MAX_ENTRIES) {
            return Err(ClientError::Availability(format!(
                "refusing to parse (availability limit — more than {} entries)",
                guards::MAX_ENTRIES
            )));
        }
        self.parse(body)
    }

    fn get<T: DeserializeOwned>(&self, path: &str, timeout: Duration) -> Result<T, ClientError> {
        let response = self.call("GET", path, b"", false, timeout)?;
        self.parse(&response.body)
    }

    fn get_list<T: DeserializeOwned>(
        &self,
        path: &str,
        timeout: Duration,
    ) -> Result<Vec<T>, ClientError> {
        let response = self.call("GET", path, b"", false, timeout)?;
        self.parse_list(&response.body)
    }

    pub fn version(&self) -> Result<Version, ClientError> {
        let response = self.call(
            "GET",
            "/version",
            b"",
            true,
            crate::transport::PROBE_TIMEOUT,
        )?;
        self.parse(&response.body)
    }

    /// Adopt the engine's `ApiVersion` as the request prefix.
    pub fn adopt_api(&mut self, version: &Version) {
        let api = version.api_version.trim();
        if !api.is_empty() {
            self.api = format!("v{api}");
        }
    }

    pub fn info(&self) -> Result<Info, ClientError> {
        self.get("/info", DEFAULT_TIMEOUT)
    }

    pub fn containers(&self) -> Result<Vec<Container>, ClientError> {
        self.get_list("/containers/json?all=1", DEFAULT_TIMEOUT)
    }

    pub fn images(&self) -> Result<Vec<crate::dto::Image>, ClientError> {
        self.get_list("/images/json", DEFAULT_TIMEOUT)
    }

    pub fn volumes(&self) -> Result<Vec<Volume>, ClientError> {
        let response: crate::dto::ListResponse<Volume> = self.get("/volumes", DEFAULT_TIMEOUT)?;
        Ok(response.volumes.unwrap_or_default())
    }

    pub fn networks(&self) -> Result<Vec<Network>, ClientError> {
        self.get_list("/networks", DEFAULT_TIMEOUT)
    }

    pub fn events(&self, since: i64, until: i64) -> Result<Vec<Event>, ClientError> {
        let path = format!("/events?since={since}&until={until}");
        let response = self.call("GET", &path, b"", false, DEFAULT_TIMEOUT)?;
        Ok(parse_events(&response.body))
    }

    pub fn df(&self) -> Result<Df, ClientError> {
        self.get("/system/df", DF_TIMEOUT)
    }

    pub fn stats(&self, id: &str) -> Result<Stats, ClientError> {
        self.get(
            &format!("/containers/{}/stats?stream=false", encode(id)),
            DEFAULT_TIMEOUT,
        )
    }

    pub fn logs(&self, id: &str, tail: usize, timestamps: bool) -> Result<String, ClientError> {
        let mut path = format!(
            "/containers/{}/logs?stdout=1&stderr=1&tail={tail}",
            encode(id)
        );
        if timestamps {
            path.push_str("&timestamps=1");
        }
        let response = self.call("GET", &path, b"", false, DEFAULT_TIMEOUT)?;
        Ok(crate::logs::demux(&response.body))
    }

    pub fn inspect(&self, id: &str) -> Result<ContainerDetail, ClientError> {
        self.get(&format!("/containers/{}/json", encode(id)), DEFAULT_TIMEOUT)
    }

    pub fn container_top(&self, id: &str) -> Result<ContainerTop, ClientError> {
        self.get(&format!("/containers/{}/top", encode(id)), DEFAULT_TIMEOUT)
    }

    pub fn container_changes(&self, id: &str) -> Result<Vec<FsChange>, ClientError> {
        let response = self.call(
            "GET",
            &format!("/containers/{}/changes", encode(id)),
            b"",
            false,
            DEFAULT_TIMEOUT,
        )?;
        if response.body.is_empty() || response.body == b"null" || response.body == b"null\n" {
            return Ok(Vec::new());
        }
        self.parse_list(&response.body)
    }

    pub fn image_history(&self, id: &str) -> Result<Vec<ImageLayer>, ClientError> {
        self.get_list(&format!("/images/{}/history", encode(id)), DEFAULT_TIMEOUT)
    }

    pub fn action(&self, method: &str, path: &str) -> Result<(), ClientError> {
        // A stop/restart waits out the engine's stop grace period (10 s by
        // default) before it answers; the general 5 s budget is too short.
        self.call(method, path, b"", false, Duration::from_secs(30))?;
        Ok(())
    }

    pub fn create_volume(&self, name: &str) -> Result<(), ClientError> {
        let body = serde_json::json!({ "Name": name }).to_string();
        self.call(
            "POST",
            "/volumes/create",
            body.as_bytes(),
            false,
            DEFAULT_TIMEOUT,
        )?;
        Ok(())
    }

    pub fn create_network(&self, name: &str) -> Result<(), ClientError> {
        let body = serde_json::json!({ "Name": name, "Driver": "bridge" }).to_string();
        self.call(
            "POST",
            "/networks/create",
            body.as_bytes(),
            false,
            DEFAULT_TIMEOUT,
        )?;
        Ok(())
    }

    /// Pull a reference. A bare name becomes `:latest`; the registry path is
    /// kept intact and the tag is split off the last colon unless a digest
    /// (`@sha256:...`) is present.
    pub fn pull(&self, reference: &str) -> Result<(), ClientError> {
        let (image, tag) = split_reference(reference);
        let path = if tag.is_empty() {
            format!("/images/create?fromImage={}", encode(image))
        } else {
            format!(
                "/images/create?fromImage={}&tag={}",
                encode(image),
                encode(tag)
            )
        };
        let response = self.call("POST", &path, b"", false, Duration::from_secs(120))?;
        check_pull_stream(&response.body)?;
        Ok(())
    }

    pub fn create_container(&self, body: &[u8], name: &str) -> Result<String, ClientError> {
        let path = if name.is_empty() {
            "/containers/create".to_owned()
        } else {
            format!("/containers/create?name={}", encode(name))
        };
        let response = self.call("POST", &path, body, false, DEFAULT_TIMEOUT)?;
        let parsed: CreateResponse = self.parse(&response.body)?;
        Ok(parsed.id)
    }

    /// POST a prune route and return the bytes it reclaimed.
    pub fn prune(&self, path: &str) -> Result<u64, ClientError> {
        let response = self.call("POST", path, b"", false, DEFAULT_TIMEOUT)?;
        if response.body.is_empty() {
            return Ok(0);
        }
        let value: serde_json::Value = serde_json::from_slice(&response.body)?;
        Ok(value
            .get("SpaceReclaimed")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0))
    }

    pub fn start_container(&self, id: &str) -> Result<(), ClientError> {
        self.action("POST", &format!("/containers/{}/start", encode(id)))
    }
}

#[derive(serde::Deserialize)]
struct CreateResponse {
    #[serde(rename = "Id", default)]
    id: String,
}

fn engine_message(response: &Response) -> String {
    #[derive(serde::Deserialize)]
    struct Envelope {
        #[serde(default)]
        message: String,
    }
    if let Ok(envelope) = serde_json::from_slice::<Envelope>(&response.body)
        && !envelope.message.is_empty()
    {
        return envelope.message;
    }
    let text = String::from_utf8_lossy(&response.body);
    let trimmed = text.trim();
    if trimmed.is_empty() {
        format!("HTTP {}", response.status)
    } else {
        trimmed.to_owned()
    }
}

fn check_pull_stream(body: &[u8]) -> Result<(), ClientError> {
    #[derive(serde::Deserialize)]
    struct PullDetail {
        #[serde(default)]
        message: String,
    }
    #[derive(serde::Deserialize)]
    struct PullEvent {
        #[serde(default)]
        error: String,
        #[serde(rename = "errorDetail", default)]
        error_detail: Option<PullDetail>,
    }
    for line in body.split(|&b| b == b'\n') {
        if line.is_empty() {
            continue;
        }
        if let Ok(event) = serde_json::from_slice::<PullEvent>(line) {
            if !event.error.is_empty() {
                return Err(ClientError::Engine {
                    status: 200,
                    message: event.error,
                });
            }
            if let Some(detail) = event.error_detail
                && !detail.message.is_empty()
            {
                return Err(ClientError::Engine {
                    status: 200,
                    message: detail.message,
                });
            }
        }
    }
    Ok(())
}

fn split_reference(reference: &str) -> (&str, &str) {
    if reference.contains('@') {
        return (reference, "");
    }
    match reference.rsplit_once(':') {
        Some((image, tag))
            if !image.is_empty()
                && !tag.is_empty()
                && !tag.contains('/')
                && !image.ends_with('/') =>
        {
            (image, tag)
        }
        _ => (reference, "latest"),
    }
}

/// Percent-encode one path/query component. `/` in a repository name is part of
/// the path, so it is preserved; everything else outside the unreserved set is
/// escaped.
pub fn encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(0);

    fn serve(response: &'static [u8]) -> PathBuf {
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("berth-client-{}-{id}.sock", std::process::id()));
        let listener = std::os::unix::net::UnixListener::bind(&path).expect("bind");
        std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut request = [0u8; 4096];
                let _ = stream.read(&mut request);
                let _ = stream.write_all(response);
            }
        });
        path
    }

    #[test]
    fn version_parses_and_adopts_the_prefix() {
        let path = serve(
            b"HTTP/1.1 200 OK\r\nContent-Length: 56\r\n\r\n{\"Version\":\"29.1.3\",\"ApiVersion\":\"1.56\",\"Components\":[]}",
        );
        let mut client = Client::new(path.to_string_lossy());
        let version = client.version().expect("version");
        assert_eq!(version.version, "29.1.3");
        client.adopt_api(&version);
        assert_eq!(client.api(), "v1.56");
    }

    #[test]
    fn an_engine_error_keeps_the_engine_message() {
        let path = serve(
            b"HTTP/1.1 404 Not Found\r\nContent-Length: 31\r\n\r\n{\"message\":\"no such container\"}",
        );
        let client = Client::new(path.to_string_lossy());
        let error = client.inspect("missing").expect_err("must fail");
        match error {
            ClientError::Engine { status, message } => {
                assert_eq!(status, 404);
                assert_eq!(message, "no such container");
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn a_list_over_the_cap_is_refused() {
        let mut body = String::from("[");
        for i in 0..(guards::MAX_ENTRIES + 1) {
            if i > 0 {
                body.push(',');
            }
            body.push_str("{}");
        }
        body.push(']');
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        );
        let leaked: &'static [u8] = Box::leak(response.into_bytes().into_boxed_slice());
        let path = serve(leaked);
        let client = Client::new(path.to_string_lossy());
        let error = client.containers().expect_err("must refuse");
        assert!(matches!(error, ClientError::Availability(_)));
    }

    #[test]
    fn a_deep_document_is_refused() {
        let body = format!("{}{}", "[".repeat(65), "]".repeat(65));
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        );
        let leaked: &'static [u8] = Box::leak(response.into_bytes().into_boxed_slice());
        let path = serve(leaked);
        let client = Client::new(path.to_string_lossy());
        let error = client.containers().expect_err("must refuse");
        assert!(matches!(error, ClientError::Availability(_)));
    }

    #[test]
    fn references_split_and_encode() {
        assert_eq!(split_reference("nginx"), ("nginx", "latest"));
        assert_eq!(split_reference("nginx:alpine"), ("nginx", "alpine"));
        assert_eq!(split_reference("ghcr.io/a/b:1.2"), ("ghcr.io/a/b", "1.2"));
        assert_eq!(
            split_reference("alpine@sha256:123456"),
            ("alpine@sha256:123456", "")
        );
        assert_eq!(encode("a b/c:d"), "a%20b/c%3Ad");
        assert!(check_pull_stream(br#"{"status":"Downloading"}"#).is_ok());
        assert!(check_pull_stream(br#"{"error":"pull access denied"}"#).is_err());
    }
}
