//! Disposable live-engine action smoke. It creates a uniquely named alpine
//! container, starts/stops/restarts/deletes it, and prints each engine answer.
//! The container is removed even when a step fails.

use berth_engine::client::Client;

fn main() -> anyhow::Result<()> {
    let runtimes = berth_engine::discovery::scan("");
    let runtime = runtimes
        .iter()
        .find(|runtime| runtime.reachable)
        .ok_or_else(|| anyhow::anyhow!("no engine answering"))?;
    let client = Client::new(runtime.socket.clone())
        .with_api(runtime.api.clone())
        .with_dialect(runtime.dialect);
    let name = format!("berth-live-smoke-{}", std::process::id());
    let body = serde_json::json!({
        "Image": "alpine:3.22",
        "Cmd": ["sleep", "120"],
    });
    let id = client.create_container(&serde_json::to_vec(&body)?, &name)?;
    println!("created {name} {id}");
    let result = (|| -> anyhow::Result<()> {
        client.start_container(&id)?;
        println!("started");
        client.action("POST", &format!("/containers/{id}/restart"))?;
        println!("restarted");
        client.action("POST", &format!("/containers/{id}/pause"))?;
        println!("paused");
        client.action("POST", &format!("/containers/{id}/unpause"))?;
        println!("resumed");
        client.action("POST", &format!("/containers/{id}/stop"))?;
        println!("stopped");
        Ok(())
    })();
    let _ = client.action("DELETE", &format!("/containers/{id}?force=1"));
    println!("deleted {name}");
    result
}
