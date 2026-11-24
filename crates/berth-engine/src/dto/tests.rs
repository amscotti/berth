use super::*;

#[test]
fn a_container_entry_parses_with_its_api_spelling() {
    let json = r#"{
        "Id": "abc123",
        "Names": ["/web"],
        "Image": "nginx:alpine",
        "State": "running",
        "Status": "Up 2 hours",
        "Labels": {"com.docker.compose.project": "shop"},
        "Ports": [{"PrivatePort": 80, "PublicPort": 8080, "Type": "tcp", "IP": "0.0.0.0"}]
    }"#;
    let container: Container = serde_json::from_str(json).expect("parse");
    assert_eq!(container.id, "abc123");
    assert_eq!(container.name(), "web");
    assert_eq!(container.project(), Some("shop"));
    assert!(container.is_up());
    assert_eq!(container.ports[0].public_port, Some(8080));
}

#[test]
fn missing_fields_default_instead_of_failing() {
    let container: Container = serde_json::from_str("{}").expect("parse");
    assert_eq!(container.name(), "");
    assert!(!container.is_up());
    let image: Image = serde_json::from_str("{}").expect("parse");
    assert!(image.dangling());
    assert_eq!(image.label(), "untagged");
}

#[test]
fn actions_follow_the_state_the_engine_sees() {
    // (state, start, stop, restart, pause, resume, terminal)
    for (state, start, stop, restart, pause, resume, terminal) in [
        ("running", false, true, true, true, false, true),
        ("paused", false, true, false, false, true, false),
        ("restarting", false, true, false, false, false, false),
        ("created", true, false, false, false, false, false),
        ("exited", true, false, false, false, false, false),
        ("dead", true, false, false, false, false, false),
        ("removing", false, false, false, false, false, false),
        ("", false, false, false, false, false, false),
    ] {
        let actions = Actions::for_state(state);
        assert_eq!(
            (
                actions.start,
                actions.stop,
                actions.restart,
                actions.pause,
                actions.resume,
                actions.terminal
            ),
            (start, stop, restart, pause, resume, terminal),
            "{state}"
        );
    }
}

#[test]
fn podman_is_detected_from_the_version_components() {
    let version: Version = serde_json::from_str(
        r#"{"Version":"5.0.0","Components":[{"Name":"Podman Engine","Version":"5.0.0"}]}"#,
    )
    .expect("parse");
    assert!(version.is_podman());
}

#[test]
fn a_network_counts_attached_containers() {
    let network: Network =
        serde_json::from_str(r#"{"Name":"bridge","Id":"n1","Containers":{"a":{},"b":{}}}"#)
            .expect("parse");
    assert_eq!(network.attached(), 2);
    assert!(network.builtin());
}
#[test]
fn top_changes_and_image_layers_parse_their_api_shapes() {
    let top: ContainerTop = serde_json::from_str(
        r#"{"Titles":["PID","USER","COMMAND"],"Processes":[["1","root","nginx"]]}"#,
    )
    .expect("top");
    assert_eq!(top.titles[0], "PID");
    assert_eq!(top.processes[0][2], "nginx");

    let changes: Vec<FsChange> =
        serde_json::from_str(r#"[{"Path":"/run/x","Kind":1},{"Path":"/etc/y","Kind":0}]"#)
            .expect("changes");
    assert_eq!(changes.len(), 2);
    assert_eq!(changes[0].kind, 1);

    let layers: Vec<ImageLayer> = serde_json::from_str(
        r#"[{"Id":"sha256:a","Created":1,"CreatedBy":"cmd","Size":40,"Comment":""}]"#,
    )
    .expect("layers");
    assert_eq!(layers[0].id, "sha256:a");
    assert_eq!(layers[0].size, 40);
}

#[test]
fn stats_json_parses_real_engine_snake_case_cpu_usage() {
    let json = r#"{
        "cpu_stats": {
            "cpu_usage": {
                "total_usage": 3000000,
                "percpu_usage": [1500000, 1500000],
                "usage_in_kernelmode": 500000,
                "usage_in_usermode": 2500000
            },
            "system_cpu_usage": 40000000,
            "online_cpus": 4
        },
        "precpu_stats": {
            "cpu_usage": {
                "total_usage": 1000000,
                "percpu_usage": [500000, 500000],
                "usage_in_kernelmode": 200000,
                "usage_in_usermode": 800000
            },
            "system_cpu_usage": 20000000,
            "online_cpus": 4
        },
        "memory_stats": {
            "usage": 1000,
            "limit": 2000,
            "stats": { "inactive_file": 200 }
        },
        "pids_stats": { "current": 5 }
    }"#;
    let parsed: Stats = serde_json::from_str(json).expect("stats");
    assert_eq!(parsed.cpu_stats.cpu_usage.total_usage, 3_000_000);
    assert_eq!(
        parsed.cpu_stats.cpu_usage.percpu_usage,
        vec![1_500_000, 1_500_000]
    );
    assert_eq!(parsed.cpu_stats.cpu_usage.usage_in_kernelmode, 500_000);
    assert_eq!(parsed.cpu_stats.cpu_usage.usage_in_usermode, 2_500_000);
    assert_eq!(parsed.precpu_stats.cpu_usage.total_usage, 1_000_000);

    let sample = crate::stats::Sample::from_stats(&parsed);
    assert!(sample.cpu_known);
    assert!((sample.cpu - 40.0).abs() < 0.001, "got {}", sample.cpu);
    assert_eq!(sample.mem, 800);
    assert_eq!(sample.pids, Some(5));
}
