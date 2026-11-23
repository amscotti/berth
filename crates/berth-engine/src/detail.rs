//! Container inspect facts the list endpoint tells only in prose: the exact
//! start time, the restart count, and a `docker run` line reproducing the
//! container. The run command is best-effort; secrets are redacted by key.

use chrono::DateTime;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::dto::{HostConfig, Mount};

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

/// One container mount entry summarized for the inspector.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MountSummary {
    pub kind: String,
    pub source: String,
    pub destination: String,
    pub rw: bool,
}

/// What the inspect document says that the list doesn't: started-at, exit and
/// health, restarts, environment, mounts, addresses, and the run command.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct InspectFacts {
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

impl InspectFacts {
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
        let secret = is_secret(key, value);
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

    // How the container is kept running, and where it runs.
    if let Some(policy) = &detail.host_config.restart_policy {
        match policy.name.trim() {
            "" | "no" => {}
            "on-failure" if policy.maximum_retry_count > 0 => {
                out.push_str(" --restart on-failure:");
                out.push_str(&policy.maximum_retry_count.to_string());
            }
            other => {
                out.push_str(" --restart ");
                out.push_str(&quote(other));
            }
        }
    }
    let network = detail.host_config.network_mode.trim();
    // `default` is what `docker run` does unaided.
    if !network.is_empty() && network != "default" {
        out.push_str(" --network ");
        out.push_str(&quote(network));
    }
    let user = detail.config.user.trim();
    if !user.is_empty() {
        out.push_str(" --user ");
        out.push_str(&quote(user));
    }
    let workdir = detail.config.working_dir.trim();
    if !workdir.is_empty() {
        out.push_str(" -w ");
        out.push_str(&quote(workdir));
    }
    // `docker run` takes an entrypoint as one word. A container's can be
    // several; the first is the entrypoint and the rest are what it is
    // given, in front of the command. Placed that way, the copy runs the
    // same words in the same order.
    let entrypoint = words(&detail.config.entrypoint);
    if let Some(first) = entrypoint.first() {
        out.push_str(" --entrypoint ");
        out.push_str(&quote(first));
    }

    append_ports(&mut out, detail.host_config.port_bindings.as_ref());
    append_mounts(&mut out, detail.mounts.as_deref());
    append_env(&mut out, detail.config.env.as_deref(), redacted);
    append_labels(&mut out, detail.config.labels.as_ref());

    out.push(' ');
    out.push_str(&quote(image));
    for argument in entrypoint
        .iter()
        .skip(1)
        .copied()
        .chain(words(&detail.config.cmd))
    {
        out.push(' ');
        out.push_str(&quote(argument));
    }
    Some(out)
}

/// The words of a list the engine may leave out or leave empty.
fn words(list: &Option<Vec<String>>) -> Vec<&str> {
    list.iter()
        .flatten()
        .map(String::as_str)
        .filter(|word| !word.is_empty())
        .collect()
}

fn append_labels(out: &mut String, labels: Option<&Value>) {
    let Some(map) = labels.and_then(Value::as_object) else {
        return;
    };
    let mut pairs: Vec<(&String, &Value)> = map.iter().collect();
    // In one order whatever order the engine listed them, so the same
    // container always copies to the same command.
    pairs.sort_by_key(|(key, _)| *key);
    for (key, value) in pairs {
        if key.is_empty() || !value.is_string() {
            continue;
        }
        out.push_str(" --label ");
        out.push_str(&quote(&format!("{key}={}", value.as_str().unwrap_or(""))));
    }
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
        // A named volume is mounted by its name, as it was created: its
        // directory inside the engine's own storage isn't a path on the
        // host, and bind-mounting it would be another kind of mount.
        let from = mount
            .name
            .as_deref()
            .filter(|name| !name.is_empty())
            .unwrap_or(mount.source.as_str());
        if from.is_empty() || mount.destination.is_empty() {
            continue;
        }
        out.push_str(" -v ");
        out.push_str(&quote(&format!(
            "{}:{}{}",
            from,
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
        let value = if is_secret(key, value) {
            *redacted = true;
            "<redacted>"
        } else {
            value
        };
        out.push_str(" -e ");
        out.push_str(&quote(&format!("{key}={value}")));
    }
}

/// Whether an environment value is shown redacted: the key says it is a
/// credential, or the value is a URL that carries one.
fn is_secret(key: &str, value: &str) -> bool {
    looks_secret(key) || has_userinfo(value)
}

/// True for `scheme://user:password@host`. A bare `user@host` is a name, not a
/// credential, so only an at-sign with a colon before it counts.
fn has_userinfo(value: &str) -> bool {
    let Some((_, rest)) = value.split_once("://") else {
        return false;
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    authority
        .rsplit_once('@')
        .is_some_and(|(userinfo, _)| userinfo.contains(':'))
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
mod tests;
