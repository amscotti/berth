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
    use berth_engine::test_support::{MockEngine, Reply};

    #[test]
    fn reads_logs_and_inspect_and_names_the_id() {
        let engine = MockEngine::start();
        engine
            .on("/logs")
            .reply(Reply::framed("hello\n"))
            .on("/json")
            .ok(r#"{"Id":"abc","Name":"/web","State":{"StartedAt":"2026-11-01T19:55:00Z"}}"#);
        let outcome = load(Request {
            socket: engine.socket(),
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
