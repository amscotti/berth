//! Container inspect facts the list endpoint tells only in prose: the exact
//! start time, the restart count, and a `docker run` line reproducing the
//! container. The run command is best-effort; secrets are redacted by key.

use chrono::DateTime;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "PascalCase")]
pub struct ContainerDetail {
    #[serde(rename = "Id")]
    pub id: String,
    pub name: String,
    pub created: String,
    pub path: String,
    pub args: Vec<String>,
    pub state: DetailState,
    pub config: DetailConfig,
    pub host_config: HostConfig,
    pub mounts: Option<Vec<Mount>>,
    pub network_settings: Option<Value>,
    pub restart_count: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "PascalCase")]
pub struct DetailState {
    pub status: String,
    pub running: bool,
    pub paused: bool,
    pub restarting: bool,
    #[serde(rename = "OOMKilled")]
    pub oom_killed: bool,
    pub dead: bool,
    pub pid: u32,
    pub exit_code: i32,
    pub error: String,
    pub started_at: String,
    pub finished_at: String,
    pub health: Option<Health>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "PascalCase")]
pub struct Health {
    pub status: String,
    pub failing_streak: i32,
    pub log: Option<Value>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "PascalCase")]
pub struct DetailConfig {
    pub hostname: String,
    pub image: String,
    pub user: String,
    pub working_dir: String,
    pub tty: bool,
    pub env: Option<Vec<String>>,
    pub cmd: Option<Vec<String>>,
    pub entrypoint: Option<Vec<String>>,
    pub labels: Option<Value>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "PascalCase")]
pub struct HostConfig {
    pub network_mode: String,
    pub port_bindings: Option<Value>,
    pub restart_policy: Option<RestartPolicy>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "PascalCase")]
pub struct RestartPolicy {
    pub name: String,
    pub maximum_retry_count: i32,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "PascalCase")]
pub struct Mount {
    #[serde(rename = "Type")]
    pub mount_type: String,
    pub name: Option<String>,
    pub source: String,
    pub destination: String,
    #[serde(rename = "RW")]
    pub rw: bool,
}

/// One container mount entry summarized for the inspector.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MountSummary {
    pub kind: String,
    pub source: String,
    pub destination: String,
    pub rw: bool,
}

/// Started-at, exit/health diagnostics, restart count, environment, mounts, IPs, and run command.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Lifetime {
    pub started_at: Option<i64>,
    pub restarts: u32,
    pub exit_code: Option<i32>,
    pub oom_killed: bool,
    pub restart_policy: Option<String>,
    pub health_log: Option<String>,
    pub command: Option<String>,
    pub workdir: Option<String>,
    pub env: Vec<(String, String, bool)>,
    pub mounts: Vec<MountSummary>,
    pub ips: Vec<(String, String)>,
    pub run_cmd: Option<String>,
    pub run_redacted: bool,
}

impl Lifetime {
    /// Build from a raw inspect document. A malformed document is not fatal:
    /// the pane simply has no extra facts.
    pub fn from_inspect(raw: &[u8]) -> Option<Self> {
        if crate::guards::nesting_exceeds(raw, crate::guards::MAX_DEPTH) {
            return None;
        }
        let detail: ContainerDetail = serde_json::from_slice(raw).ok()?;
        Some(Self::from_detail(&detail))
    }

    pub fn from_detail(detail: &ContainerDetail) -> Self {
        let restarts = u32::try_from(detail.restart_count.max(0)).unwrap_or(u32::MAX);
        let mut redacted = false;
        let run_cmd = run_command(detail, &mut redacted);
        let run_redacted = run_cmd.is_some() && redacted;
        let has_finished = !detail.state.finished_at.is_empty()
            && !detail.state.finished_at.starts_with("0001-01-01");
        let exit_code = (!detail.state.running
            && (has_finished
                || detail.state.exit_code != 0
                || matches!(detail.state.status.as_str(), "exited" | "dead")))
        .then_some(detail.state.exit_code);
        let restart_policy = detail
            .host_config
            .restart_policy
            .as_ref()
            .and_then(|policy| {
                let name = policy.name.trim();
                if name.is_empty() || name == "no" {
                    None
                } else if name == "on-failure" && policy.maximum_retry_count > 0 {
                    Some(format!("on-failure:{}", policy.maximum_retry_count))
                } else {
                    Some(name.to_owned())
                }
            });
        let health_log = extract_health_log(detail.state.health.as_ref());
        let env = extract_env(detail.config.env.as_deref());
        let mounts = extract_mounts(detail.mounts.as_deref());
        let ips = extract_ips(detail.network_settings.as_ref());
        let command = extract_command(&detail.config);
        let workdir = (!detail.config.working_dir.trim().is_empty())
            .then(|| detail.config.working_dir.trim().to_owned());
        Self {
            started_at: rfc3339(&detail.state.started_at),
            restarts,
            exit_code,
            oom_killed: detail.state.oom_killed,
            restart_policy,
            health_log,
            command,
            workdir,
            env,
            mounts,
            ips,
            run_cmd,
            run_redacted,
        }
    }
}

fn extract_health_log(health: Option<&Health>) -> Option<String> {
    let health = health?;
    if health.failing_streak <= 0 && health.status != "unhealthy" {
        return None;
    }
    let last = health.log.as_ref()?.as_array()?.last()?;
    let output = last
        .get("Output")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim();
    if output.is_empty() {
        return None;
    }
    let first_line = output.lines().next().unwrap_or(output).trim();
    Some(first_line.chars().take(160).collect())
}

fn extract_env(env: Option<&[String]>) -> Vec<(String, String, bool)> {
    let Some(env) = env else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in env {
        let (key, value) = entry.split_once('=').unwrap_or((entry, ""));
        if key.is_empty() || key.starts_with("com.docker.compose.") {
            continue;
        }
        let secret = looks_secret(key);
        let display = if secret {
            "<redacted>".to_owned()
        } else {
            value.to_owned()
        };
        out.push((key.to_owned(), display, secret));
    }
    out
}

fn extract_mounts(mounts: Option<&[Mount]>) -> Vec<MountSummary> {
    let Some(mounts) = mounts else {
        return Vec::new();
    };
    mounts
        .iter()
        .filter(|m| !m.destination.is_empty())
        .map(|m| MountSummary {
            kind: if m.mount_type.is_empty() {
                "mount".to_owned()
            } else {
                m.mount_type.clone()
            },
            source: m.name.clone().unwrap_or_else(|| m.source.clone()),
            destination: m.destination.clone(),
            rw: m.rw,
        })
        .collect()
}

fn extract_ips(network_settings: Option<&Value>) -> Vec<(String, String)> {
    let Some(networks) = network_settings
        .and_then(|ns| ns.get("Networks"))
        .and_then(Value::as_object)
    else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (name, net) in networks {
        let ip = net
            .get("IPAddress")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim();
        if !ip.is_empty() {
            out.push((name.clone(), ip.to_owned()));
        }
    }
    out
}

fn extract_command(config: &DetailConfig) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(ep) = &config.entrypoint {
        parts.extend(ep.iter().filter(|s| !s.is_empty()).cloned());
    }
    if let Some(cmd) = &config.cmd {
        parts.extend(cmd.iter().filter(|s| !s.is_empty()).cloned());
    }
    (!parts.is_empty()).then(|| parts.join(" "))
}

/// Seconds the container has been up, from a status like "Up 2 hours (healthy)".
pub fn up_seconds(status: &str) -> Option<i64> {
    let rest = status.strip_prefix("Up")?;
    let rest = rest.strip_prefix(' ')?;
    let lower = rest.to_ascii_lowercase();
    if lower.starts_with("about a minute") {
        return Some(60);
    }
    if lower.starts_with("about an hour") {
        return Some(3600);
    }
    if lower.starts_with("less than a second") {
        return Some(0);
    }

    let mut seconds = 0i64;
    let mut any = false;
    let mut tokens = rest.split_whitespace();
    while let Some(number) = tokens.next() {
        let Ok(value) = number.parse::<i64>() else {
            continue;
        };
        let unit = tokens.next()?;
        let unit = unit.trim_end_matches([',', ')']);
        let multiplier = match unit {
            "second" | "seconds" => 1,
            "minute" | "minutes" => 60,
            "hour" | "hours" => 3600,
            "day" | "days" => 86_400,
            "week" | "weeks" => 7 * 86_400,
            "month" | "months" => 30 * 86_400,
            "year" | "years" => 365 * 86_400,
            _ => return None,
        };
        seconds = seconds.saturating_add(value.saturating_mul(multiplier));
        any = true;
    }
    any.then_some(seconds)
}

fn rfc3339(text: &str) -> Option<i64> {
    let stamp = DateTime::parse_from_rfc3339(text).ok()?.timestamp();
    (stamp > 0).then_some(stamp)
}

fn run_command(detail: &ContainerDetail, redacted: &mut bool) -> Option<String> {
    let image = detail.config.image.trim();
    if image.is_empty() {
        return None;
    }
    let mut out = String::from("docker run -d");
    let name = detail.name.trim_start_matches('/');
    if !name.is_empty() {
        out.push_str(" --name ");
        out.push_str(&quote(name));
    }

    append_ports(&mut out, detail.host_config.port_bindings.as_ref());
    append_mounts(&mut out, detail.mounts.as_deref());
    append_env(&mut out, detail.config.env.as_deref(), redacted);

    out.push(' ');
    out.push_str(&quote(image));
    Some(out)
}

fn append_ports(out: &mut String, bindings: Option<&Value>) {
    let Some(map) = bindings.and_then(Value::as_object) else {
        return;
    };
    for (container_port, hosts) in map {
        let Some(hosts) = hosts.as_array() else {
            continue;
        };
        for host in hosts {
            let host_port = host
                .get("HostPort")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if host_port.is_empty() {
                continue;
            }
            let host_ip = host
                .get("HostIp")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let container = container_port
                .strip_suffix("/tcp")
                .unwrap_or(container_port);
            out.push_str(" -p ");
            if !host_ip.is_empty() && !matches!(host_ip, "0.0.0.0" | "::") {
                out.push_str(host_ip);
                out.push(':');
            }
            out.push_str(host_port);
            out.push(':');
            out.push_str(container);
        }
    }
}

fn append_mounts(out: &mut String, mounts: Option<&[Mount]>) {
    let Some(mounts) = mounts else {
        return;
    };
    for mount in mounts {
        if mount.source.is_empty() || mount.destination.is_empty() {
            continue;
        }
        out.push_str(" -v ");
        out.push_str(&quote(&format!(
            "{}:{}{}",
            mount.source,
            mount.destination,
            if mount.rw { "" } else { ":ro" }
        )));
    }
}

fn append_env(out: &mut String, env: Option<&[String]>, redacted: &mut bool) {
    let Some(env) = env else {
        return;
    };
    for entry in env {
        let (key, value) = entry.split_once('=').unwrap_or((entry, ""));
        if key.is_empty() {
            continue;
        }
        // Compose-injected annotations are not part of a runnable command.
        if key.starts_with("com.docker.compose.") {
            continue;
        }
        let value = if looks_secret(key) {
            *redacted = true;
            "<redacted>"
        } else {
            value
        };
        out.push_str(" -e ");
        out.push_str(&quote(&format!("{key}={value}")));
    }
}

fn looks_secret(key: &str) -> bool {
    let upper = key.to_ascii_uppercase();
    [
        "PASSWORD",
        "PASSCODE",
        "PASSPHRASE",
        "PASSWD",
        "SECRET",
        "TOKEN",
        "API_KEY",
        "APIKEY",
        "ACCESS_KEY",
        "PRIVATE_KEY",
    ]
    .iter()
    .any(|needle| upper.contains(needle))
}

/// Single-quote one word for a POSIX shell.
pub fn quote(text: &str) -> String {
    if !text.is_empty()
        && text
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || "_-./:=@".contains(ch))
    {
        return text.to_owned();
    }
    let escaped = text.replace('\'', "'\\''");
    format!("'{escaped}'")
}

#[cfg(test)]
mod tests {
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
        let life = Lifetime::from_inspect(raw).expect("parse");
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
        let life = Lifetime::from_inspect(raw).expect("parse");
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
        let life = Lifetime::from_inspect(raw).expect("parse");
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
}
