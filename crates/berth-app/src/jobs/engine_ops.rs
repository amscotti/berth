//! Machine commands: start, stop and restart a Colima, Lima or Podman
//! machine. These run the machine's own command-line tool, which can take
//! minutes and must never be left waiting on a prompt, so each command runs
//! without a stdin and under a limit. The worker owns every string and returns
//! one sentence for the toast.

use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use berth_engine::discovery::{Kind, podman_machine_name};

use super::mutation::Outcome;

/// A command to the machine behind an engine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EngineOp {
    Start,
    Stop,
    Restart,
}

impl EngineOp {
    /// The word the machine's own tool takes.
    pub(crate) fn word(self) -> &'static str {
        match self {
            EngineOp::Start => "start",
            EngineOp::Stop => "stop",
            EngineOp::Restart => "restart",
        }
    }

    /// What a button, and a confirmation, call it.
    pub(crate) fn label(self) -> &'static str {
        match self {
            EngineOp::Start => "Start",
            EngineOp::Stop => "Stop",
            EngineOp::Restart => "Restart",
        }
    }

    /// What it is doing, to name it in a sentence: "stopping".
    pub(crate) fn doing(self) -> &'static str {
        match self {
            EngineOp::Start => "starting",
            EngineOp::Stop => "stopping",
            EngineOp::Restart => "restarting",
        }
    }
}

/// How long a machine command may run before berth stops waiting for it.
const LIMIT: Duration = Duration::from_secs(300);
/// How often the runner checks whether a machine command has finished.
const POLL: Duration = Duration::from_millis(25);
/// Where the machine tools live when the app wasn't started from a shell
/// that knows about them.
const DEFAULT_PATH: &str =
    "/opt/homebrew/bin:/usr/local/bin:/opt/podman/bin:/usr/bin:/bin:/usr/sbin:/sbin";

/// Run `op` on the machine behind `socket`.
pub(super) fn run(op: EngineOp, kind: Kind, socket: &str) -> Outcome {
    let verb = op.word();
    let steps = match plan(op, kind, socket) {
        Ok(steps) => steps,
        Err(message) => {
            return Outcome {
                message: message.to_owned(),
                bad: true,
            };
        }
    };
    let path = match std::env::var("PATH") {
        Ok(existing) if !existing.is_empty() => format!("{DEFAULT_PATH}:{existing}"),
        _ => DEFAULT_PATH.to_owned(),
    };
    for step in steps {
        let Some((program, args)) = step.split_first() else {
            continue;
        };
        let mut command = Command::new(program);
        command.args(args).env("PATH", &path);
        match run_bounded(command, LIMIT) {
            Ok((true, _)) => {}
            Ok((false, stderr)) => {
                return Outcome::bad(format!(
                    "{program} {verb} failed: {}",
                    stderr.lines().next().unwrap_or("no output")
                ));
            }
            Err(CommandFailure::TimedOut) => {
                return Outcome::bad(format!(
                    "{program} {verb} did not finish within {} minutes; the machine may still be working",
                    LIMIT.as_secs() / 60
                ));
            }
            Err(CommandFailure::Io(error)) => {
                return Outcome::bad(format!("could not run {program}: {error}"));
            }
        }
    }
    Outcome {
        message: format!("{verb} engine"),
        bad: false,
    }
}

/// The commands a machine operation runs, in order. Lima and Podman restart
/// by stopping, then starting. `Err` when the engine has no machine command.
pub(super) fn plan(
    op: EngineOp,
    kind: Kind,
    socket: &str,
) -> Result<Vec<Vec<String>>, &'static str> {
    let verb = op.word();
    let words = |parts: &[&str]| {
        parts
            .iter()
            .map(|part| part.to_string())
            .collect::<Vec<_>>()
    };
    match kind {
        Kind::Colima => Ok(vec![words(&["colima", verb])]),
        Kind::Lima => {
            let name = socket
                .split("/.lima/")
                .nth(1)
                .and_then(|rest| rest.split('/').next())
                .unwrap_or("default");
            if op == EngineOp::Restart {
                Ok(vec![
                    words(&["limactl", "stop", name]),
                    words(&["limactl", "start", name]),
                ])
            } else {
                Ok(vec![words(&["limactl", verb, name])])
            }
        }
        Kind::PodmanMachine => {
            let name = podman_machine_name(socket).unwrap_or("podman-machine-default");
            if op == EngineOp::Restart {
                Ok(vec![
                    words(&["podman", "machine", "stop", name]),
                    words(&["podman", "machine", "start", name]),
                ])
            } else {
                Ok(vec![words(&["podman", "machine", verb, name])])
            }
        }
        _ => Err("this engine has no machine command"),
    }
}

/// Why a machine command gave no answer.
#[derive(Debug, PartialEq, Eq)]
enum CommandFailure {
    /// It was still running when the limit passed. It has been killed.
    TimedOut,
    Io(String),
}

/// Run a command to completion and report whether it succeeded, with its
/// stderr. It gets no stdin, so nothing can wait on a password prompt, and it
/// is killed if `limit` passes first: a machine start can take minutes, but it
/// must not hold the one mutation slot forever.
fn run_bounded(mut command: Command, limit: Duration) -> Result<(bool, String), CommandFailure> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|error| CommandFailure::Io(error.to_string()))?;
    // Drain both pipes on their own threads: a chatty command must never
    // block on a full pipe while berth waits for it to exit.
    let _stdout = drain(child.stdout.take());
    let stderr = drain(child.stderr.take());
    let deadline = Instant::now() + limit;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(POLL),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                break Err(CommandFailure::TimedOut);
            }
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                break Err(CommandFailure::Io(error.to_string()));
            }
        }
    };
    let stderr = stderr.join().unwrap_or_default();
    status.map(|status| (status.success(), stderr))
}

/// Read a pipe to its end on a new thread, and hand back what it held.
fn drain(pipe: Option<impl Read + Send + 'static>) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        let mut text = String::new();
        if let Some(mut pipe) = pipe {
            let _ = pipe.read_to_string(&mut text);
        }
        text
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn steps(parts: &[&[&str]]) -> Vec<Vec<String>> {
        parts
            .iter()
            .map(|step| step.iter().map(|word| word.to_string()).collect())
            .collect()
    }

    #[test]
    fn machine_commands_are_planned_per_kind() {
        let lima = "/Users/me/.lima/docker/sock/docker.sock";
        assert_eq!(
            plan(EngineOp::Stop, Kind::Lima, lima),
            Ok(steps(&[&["limactl", "stop", "docker"]]))
        );
        assert_eq!(
            plan(EngineOp::Restart, Kind::Lima, lima),
            Ok(steps(&[
                &["limactl", "stop", "docker"],
                &["limactl", "start", "docker"]
            ]))
        );
        // A socket outside the usual layout still names the default instance.
        assert_eq!(
            plan(EngineOp::Start, Kind::Lima, "/tmp/docker.sock"),
            Ok(steps(&[&["limactl", "start", "default"]]))
        );

        assert_eq!(
            plan(EngineOp::Restart, Kind::Colima, "/x"),
            Ok(steps(&[&["colima", "restart"]]))
        );

        let podman = "/Users/me/.local/share/containers/podman/machine/dev-vm/podman.sock";
        assert_eq!(
            plan(EngineOp::Restart, Kind::PodmanMachine, podman),
            Ok(steps(&[
                &["podman", "machine", "stop", "dev-vm"],
                &["podman", "machine", "start", "dev-vm"],
            ]))
        );
        assert_eq!(
            plan(
                EngineOp::Start,
                Kind::PodmanMachine,
                "/run/podman/podman.sock"
            ),
            Ok(steps(&[&[
                "podman",
                "machine",
                "start",
                "podman-machine-default"
            ]]))
        );

        for kind in [
            Kind::DockerDesktop,
            Kind::OrbStack,
            Kind::Rancher,
            Kind::Dockerd,
            Kind::Podman,
        ] {
            assert_eq!(
                plan(EngineOp::Stop, kind, "/x"),
                Err("this engine has no machine command"),
                "{kind:?}"
            );
        }
    }

    #[test]
    fn a_machine_command_that_never_exits_is_killed_at_its_limit() {
        let started = Instant::now();
        let mut command = Command::new("sh");
        command.args(["-c", "sleep 30"]);
        let failure = run_bounded(command, Duration::from_millis(200)).expect_err("must give up");
        assert_eq!(failure, CommandFailure::TimedOut);
        assert!(started.elapsed() < Duration::from_secs(10));
    }

    #[test]
    fn a_machine_command_reports_its_status_and_stderr() {
        let mut command = Command::new("sh");
        command.args(["-c", "echo boom >&2; exit 3"]);
        assert_eq!(
            run_bounded(command, Duration::from_secs(5)),
            Ok((false, "boom\n".to_owned()))
        );
        let mut command = Command::new("sh");
        command.args(["-c", "exit 0"]);
        assert_eq!(
            run_bounded(command, Duration::from_secs(5)),
            Ok((true, String::new()))
        );
    }

    #[test]
    fn a_machine_command_gets_no_stdin() {
        // With no stdin, a read sees end of file instead of waiting forever.
        let mut command = Command::new("sh");
        command.args(["-c", "read line; echo done"]);
        assert_eq!(
            run_bounded(command, Duration::from_secs(5)),
            Ok((true, String::new()))
        );
        let missing = run_bounded(
            Command::new("berth-no-such-program"),
            Duration::from_secs(5),
        );
        assert!(matches!(missing, Err(CommandFailure::Io(_))), "{missing:?}");
    }
}
