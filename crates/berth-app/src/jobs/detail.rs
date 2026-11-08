//! The inspector's background reads: the last 500 log lines and the engine's
//! own inspect document. One job per selection generation; a late answer is
//! dropped by the UI.

use berth_engine::client::Client;
use berth_engine::detail::ContainerDetail;
use berth_engine::dto::{ContainerTop, FsChange, ImageLayer};

#[derive(Clone, Debug)]
pub struct Request {
    pub socket: String,
    pub api: String,
    pub id: String,
    pub generation: u64,
    pub timestamps: bool,
    pub extras: bool,
}

#[derive(Clone, Debug)]
pub struct Outcome {
    pub id: String,
    pub generation: u64,
    pub logs: Result<String, String>,
    pub inspect: Result<ContainerDetail, String>,
    pub top: Option<ContainerTop>,
    pub changes: Vec<FsChange>,
}

#[derive(Clone, Debug)]
pub struct ImageOutcome {
    pub id: String,
    pub generation: u64,
    pub layers: Result<Vec<ImageLayer>, String>,
}

pub fn load(request: Request) -> Outcome {
    let client = Client::new(&request.socket).with_api(&request.api);
    let logs = client
        .logs(&request.id, 500, request.timestamps)
        .map(|text| {
            if request.timestamps {
                berth_engine::logs::localize(&text)
            } else {
                text
            }
        })
        .map_err(|error| error.message());
    let inspect = client.inspect(&request.id).map_err(|error| error.message());
    let (top, changes) = if request.extras {
        let is_running = inspect.as_ref().is_ok_and(|d| d.state.running);
        let top = if is_running {
            client.container_top(&request.id).ok()
        } else {
            None
        };
        let mut changes = client.container_changes(&request.id).unwrap_or_default();
        changes.truncate(150);
        (top, changes)
    } else {
        (None, Vec::new())
    };
    Outcome {
        id: request.id,
        generation: request.generation,
        logs,
        inspect,
        top,
        changes,
    }
}

pub fn load_image(socket: String, api: String, id: String, generation: u64) -> ImageOutcome {
    let client = Client::new(&socket).with_api(&api);
    let layers = client.image_history(&id).map_err(|error| error.message());
    ImageOutcome {
        id,
        generation,
        layers,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(0);

    fn frame(text: &str) -> Vec<u8> {
        let mut out = vec![1, 0, 0, 0];
        out.extend_from_slice(&(text.len() as u32).to_be_bytes());
        out.extend_from_slice(text.as_bytes());
        out
    }

    #[test]
    fn reads_logs_and_inspect_and_names_the_id() {
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("berth-detail-{}-{id}.sock", std::process::id()));
        let listener = std::os::unix::net::UnixListener::bind(&path).expect("bind");
        std::thread::spawn(move || {
            for _ in 0..2 {
                let (mut stream, _) = listener.accept().expect("accept");
                let mut request = [0u8; 2048];
                let n = stream.read(&mut request).unwrap_or(0);
                let text = String::from_utf8_lossy(&request[..n]);
                let body: Vec<u8> = if text.contains("/logs") {
                    frame("hello\n")
                } else {
                    br#"{"Id":"abc","Name":"/web","State":{"StartedAt":"2026-11-01T19:55:00Z"}}"#
                        .to_vec()
                };
                let head = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", body.len());
                let _ = stream.write_all(head.as_bytes());
                let _ = stream.write_all(&body);
            }
        });

        let outcome = load(Request {
            socket: path.to_string_lossy().into_owned(),
            api: "v1.43".into(),
            id: "abc".into(),
            generation: 7,
            timestamps: false,
            extras: false,
        });
        assert_eq!(outcome.id, "abc");
        assert_eq!(outcome.generation, 7);
        assert_eq!(outcome.logs.unwrap(), "hello\n");
        let detail = outcome.inspect.expect("inspect");
        assert_eq!(detail.name, "/web");
    }
}
