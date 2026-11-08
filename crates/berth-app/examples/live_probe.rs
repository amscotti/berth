//! Live-engine smoke helper: discover sockets, read one snapshot, print it.
//! No GPUI, no window. Run with `cargo run -p berth-app --example live_probe`.

fn main() -> anyhow::Result<()> {
    let runtimes = berth_engine::discovery::scan("");
    for runtime in &runtimes {
        println!(
            "{} [{}] reachable={} socket={} api={}",
            runtime.name,
            runtime.kind.label(),
            runtime.reachable,
            runtime.socket,
            runtime.api
        );
    }
    let Some(runtime) = runtimes.iter().find(|runtime| runtime.reachable) else {
        println!("no engine answering");
        return Ok(());
    };
    let client = berth_engine::client::Client::new(runtime.socket.clone())
        .with_api(runtime.api.clone())
        .with_dialect(runtime.dialect);
    let containers = client.containers()?;
    println!("containers: {}", containers.len());
    for container in &containers {
        println!(
            "  {} {} {:?} ports={}",
            container.name(),
            container.image,
            container.state,
            container.ports_text(2)
        );
    }
    println!("images: {}", client.images()?.len());
    println!("volumes: {}", client.volumes()?.len());
    println!("networks: {}", client.networks()?.len());
    if let Some(first) = containers.first() {
        let stats = client.stats(&first.id)?;
        let sample = berth_engine::stats::Sample::from_stats(&stats);
        println!(
            "stats {}: sampled={} cpu_known={} mem={} pids={:?}",
            first.name(),
            sample.sampled,
            sample.cpu_known,
            sample.mem,
            sample.pids
        );
        if let Ok(logs) = client.logs(&first.id, 10, false) {
            println!("logs: {} bytes", logs.len());
        }
    }
    Ok(())
}
