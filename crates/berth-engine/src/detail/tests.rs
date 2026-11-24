//! Tests for the inspect facts and the run command.

use super::*;

#[test]
fn uptime_reads_the_english_status() {
    assert_eq!(up_seconds("Up 18 hours"), Some(18 * 3600));
    assert_eq!(up_seconds("Up 2 hours (healthy)"), Some(2 * 3600));
    assert_eq!(up_seconds("Up 4 minutes"), Some(240));
    assert_eq!(up_seconds("Up About a minute"), Some(60));
    assert_eq!(
        up_seconds("Up 3 days, 4 hours"),
        Some(3 * 86_400 + 4 * 3600)
    );
    assert_eq!(up_seconds("Exited (0) 2 hours ago"), None);
    assert_eq!(up_seconds("Created"), None);
}

#[test]
fn inspect_yields_started_at_and_restarts() {
    let raw = br#"{
        "Id":"abc","Name":"/web","RestartCount":3,
        "State":{"StartedAt":"2026-11-01T19:55:00.123456Z"},
        "Config":{"Image":"nginx:alpine","Env":["A=1","DB_PASSWORD=hunter2"]},
        "HostConfig":{"PortBindings":{"80/tcp":[{"HostIp":"0.0.0.0","HostPort":"8080"}]}},
        "Mounts":[{"Type":"volume","Source":"data","Destination":"/data","RW":true}]
    }"#;
    let life = InspectFacts::from_inspect(raw).expect("parse");
    assert!(life.started_at.is_some());
    assert_eq!(life.restarts, 3);
    let cmd = life.run_cmd.expect("run command");
    assert!(cmd.contains("--name web"), "{cmd}");
    assert!(cmd.contains("-p 8080:80"), "{cmd}");
    assert!(cmd.contains("-v data:/data"), "{cmd}");
    assert!(cmd.contains("DB_PASSWORD=<redacted>"), "{cmd}");
    assert!(!cmd.contains("hunter2"));
    assert!(life.run_redacted);
}

#[test]
fn never_started_has_no_started_at() {
    let raw = br#"{"State":{"StartedAt":"0001-01-01T00:00:00Z"}}"#;
    let life = InspectFacts::from_inspect(raw).expect("parse");
    assert_eq!(life.started_at, None);
}

#[test]
fn quotes_only_when_needed() {
    assert_eq!(quote("nginx:alpine"), "nginx:alpine");
    assert_eq!(quote("hello world"), "'hello world'");
    assert_eq!(quote("it's"), "'it'\\''s'");
}

#[test]
fn inspect_extracts_exit_oom_restart_health_and_env() {
    let raw = br#"{
        "Id":"abc","Name":"/worker","RestartCount":2,
        "State":{
            "Status":"exited","Running":false,"OOMKilled":true,"ExitCode":137,
            "StartedAt":"2026-11-01T19:55:00Z","FinishedAt":"2026-11-01T20:00:00Z",
            "Health":{
                "Status":"unhealthy","FailingStreak":3,
                "Log":[{"ExitCode":1,"Output":"curl: (7) Failed to connect to localhost port 8080\n"}]
            }
        },
        "Config":{"Image":"worker:1","Env":["PORT=8080","API_KEY=secret123"]},
        "HostConfig":{"RestartPolicy":{"Name":"on-failure","MaximumRetryCount":5}}
    }"#;
    let life = InspectFacts::from_inspect(raw).expect("parse");
    assert_eq!(life.exit_code, Some(137));
    assert!(life.oom_killed);
    assert_eq!(life.restart_policy.as_deref(), Some("on-failure:5"));
    assert_eq!(
        life.health_log.as_deref(),
        Some("curl: (7) Failed to connect to localhost port 8080")
    );
    assert_eq!(
        life.env,
        vec![
            ("PORT".to_owned(), "8080".to_owned(), false),
            ("API_KEY".to_owned(), "<redacted>".to_owned(), true),
        ]
    );
}

#[test]
fn credentials_inside_urls_are_redacted_too() {
    let raw = br#"{
        "Name":"/db",
        "Config":{"Image":"postgres","Env":[
            "DATABASE_URL=postgres://admin:hunter2@db:5432/app",
            "SITE=https://example.com/@team",
            "LOGIN=admin@db"
        ]}
    }"#;
    let life = InspectFacts::from_inspect(raw).expect("parse");
    assert_eq!(
        life.env,
        vec![
            ("DATABASE_URL".to_owned(), "<redacted>".to_owned(), true),
            (
                "SITE".to_owned(),
                "https://example.com/@team".to_owned(),
                false
            ),
            ("LOGIN".to_owned(), "admin@db".to_owned(), false),
        ]
    );
    let command = life.run_cmd.expect("run command");
    assert!(command.contains("DATABASE_URL=<redacted>"), "{command}");
    assert!(!command.contains("hunter2"), "{command}");
    assert!(life.run_redacted);
}

/// The fixture the app's screenshots and inspector tests run on, as its
/// engine would describe it.
const FIXTURE_INSPECT: &[u8] = br#"{
    "Name":"/berth-demo-web",
    "Config":{
        "Image":"nginx:alpine",
        "User":"",
        "WorkingDir":"/usr/share/nginx/html",
        "Env":["NGINX_HOST=localhost","NGINX_PORT=80","API_SECRET_TOKEN=hunter2"],
        "Entrypoint":["/docker-entrypoint.sh"],
        "Cmd":["nginx","-g","daemon off;"],
        "Labels":{"com.docker.compose.service":"web","com.docker.compose.project":"shop"}
    },
    "HostConfig":{
        "NetworkMode":"shop_default",
        "RestartPolicy":{"Name":"unless-stopped","MaximumRetryCount":0},
        "PortBindings":{"80/tcp":[{"HostIp":"","HostPort":"8080"}]}
    },
    "Mounts":[{"Type":"volume","Name":"shop-data",
               "Source":"/var/lib/docker/volumes/shop-data/_data",
               "Destination":"/var/www/html","RW":true}]
}"#;

#[test]
fn the_run_command_reproduces_the_fixture_container() {
    let facts = InspectFacts::from_inspect(FIXTURE_INSPECT).expect("parse");
    assert_eq!(
        facts.run_cmd.as_deref(),
        Some(concat!(
            "docker run -d --name berth-demo-web --restart unless-stopped",
            " --network shop_default -w /usr/share/nginx/html",
            " --entrypoint /docker-entrypoint.sh -p 8080:80",
            " -v shop-data:/var/www/html",
            " -e NGINX_HOST=localhost -e NGINX_PORT=80",
            " -e 'API_SECRET_TOKEN=<redacted>'",
            " --label com.docker.compose.project=shop",
            " --label com.docker.compose.service=web",
            " nginx:alpine nginx -g 'daemon off;'"
        )),
        "the copy says everything the container runs with, and no secret"
    );
    assert!(facts.run_redacted, "the secret is not copied");
}

#[test]
fn a_run_command_carries_what_the_container_runs_as_and_with() {
    let raw = br#"{
        "Name":"/worker",
        "Config":{
            "Image":"busybox","User":"1000:1000","WorkingDir":"/work",
            "Entrypoint":["/bin/sh","-c"],"Cmd":["sleep 1 && echo it's done"],
            "Labels":{"b":"2","a":"1"}
        },
        "HostConfig":{
            "NetworkMode":"default",
            "RestartPolicy":{"Name":"on-failure","MaximumRetryCount":3}
        },
        "Mounts":[{"Type":"bind","Source":"/etc/hostname","Destination":"/etc/hostname","RW":false}]
    }"#;
    let facts = InspectFacts::from_inspect(raw).expect("parse");
    assert_eq!(
        facts.run_cmd.as_deref(),
        Some(concat!(
            "docker run -d --name worker --restart on-failure:3 --user 1000:1000",
            " -w /work --entrypoint /bin/sh",
            " -v /etc/hostname:/etc/hostname:ro",
            " --label a=1 --label b=2",
            " busybox -c 'sleep 1 && echo it'\\''s done'"
        )),
        "a bind mount stays a path, the default network is left out, and an \
         entrypoint of several words runs in order after the image"
    );
    assert!(!facts.run_redacted);
}

#[test]
fn run_command_brackets_ipv6_keeps_ephemeral_ports_and_empty_args() {
    let raw = br#"{
        "Name":"/edge",
        "Config":{
            "Image":"app:1",
            "Entrypoint":[""],
            "Cmd":["--prefix",""]
        },
        "HostConfig":{
            "PortBindings":{
                "80/tcp":[{"HostIp":"::1","HostPort":"8080"}],
                "443/tcp":[{"HostIp":"","HostPort":""}],
                "53/udp":[{"HostIp":"127.0.0.1","HostPort":""}]
            }
        }
    }"#;
    let facts = InspectFacts::from_inspect(raw).expect("parse");
    let cmd = facts.run_cmd.expect("run command");
    assert!(cmd.contains(" --entrypoint '' "), "{cmd}");
    assert!(cmd.contains(" -p [::1]:8080:80"), "{cmd}");
    assert!(cmd.contains(" -p 443"), "{cmd}");
    assert!(cmd.contains(" -p 127.0.0.1::53/udp"), "{cmd}");
    assert!(cmd.ends_with(" app:1 --prefix ''"), "{cmd}");
}
