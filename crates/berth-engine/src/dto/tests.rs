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
