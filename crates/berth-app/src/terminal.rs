//! Opening a container shell in the user's terminal, pinned to the socket the
//! window is reading.

use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use berth_engine::discovery::Kind;

/// How long macOS AppleScript may run before berth gives up waiting on it.
const OSASCRIPT_TIMEOUT: Duration = Duration::from_secs(10);
/// How often the runner checks whether `osascript` has exited.
const POLL: Duration = Duration::from_millis(25);

fn cli(kind: Kind) -> (&'static str, &'static str) {
    match kind {
        Kind::Podman | Kind::PodmanMachine => ("podman", "--url"),
        _ => ("docker", "-H"),
    }
}

pub fn exec_command(kind: Kind, socket: &str, name: &str) -> String {
    let (program, flag) = cli(kind);
    format!(
        "{program} {flag} {} exec -it {} sh",
        quote(&format!("unix://{socket}")),
        quote(name)
    )
}

pub fn open(kind: Kind, socket: &str, name: &str) -> Result<(), String> {
    let command = exec_command(kind, socket, name);
    if cfg!(target_os = "macos") {
        let script = format!(
            "tell application \"Terminal\" to do script {}",
            applescript_quote(&command)
        );
        let mut osascript = Command::new("osascript");
        osascript
            .arg("-e")
            .arg("tell application \"Terminal\" to activate")
            .arg("-e")
            .arg(script);
        return match run_bounded(osascript, OSASCRIPT_TIMEOUT) {
            Ok((true, _)) => Ok(()),
            Ok((false, stderr)) => Err(format!(
                "could not open Terminal: {}",
                stderr.lines().next().unwrap_or("osascript failed")
            )),
            Err(BoundedError::TimedOut) => {
                Err("could not open Terminal: timed out waiting for AppleScript".to_owned())
            }
            Err(BoundedError::Io(error)) => Err(format!("could not open Terminal: {error}")),
        };
    }
    for program in ["x-terminal-emulator", "gnome-terminal", "konsole", "xterm"] {
        let result = if program == "gnome-terminal" {
            Command::new(program)
                .arg("--")
                .arg("sh")
                .arg("-c")
                .arg(&command)
                .spawn()
        } else {
            Command::new(program)
                .arg("-e")
                .arg("sh")
                .arg("-c")
                .arg(&command)
                .spawn()
        };
        if result.is_ok() {
            return Ok(());
        }
    }
    Err("could not find a terminal".to_owned())
}

/// Single-quote one word for sh.
pub fn quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

/// Double-quote a string literal for AppleScript.
pub fn applescript_quote(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

#[derive(Debug, PartialEq, Eq)]
enum BoundedError {
    TimedOut,
    Io(String),
}

/// Run a short helper command with no stdin and a hard wall-clock budget.
fn run_bounded(mut command: Command, limit: Duration) -> Result<(bool, String), BoundedError> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|error| BoundedError::Io(error.to_string()))?;
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
                break Err(BoundedError::TimedOut);
            }
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                break Err(BoundedError::Io(error.to_string()));
            }
        }
    };
    match status {
        Ok(status) => {
            let stderr = stderr.join().unwrap_or_default();
            Ok((status.success(), stderr))
        }
        Err(error) => Err(error),
    }
}

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

    #[test]
    fn a_docker_command_pins_the_socket_and_name() {
        let command = exec_command(Kind::Lima, "/tmp/a b.sock", "web");
        assert!(command.starts_with("docker -H 'unix:///tmp/a b.sock'"));
        assert!(command.contains("exec -it 'web' sh"));
    }

    #[test]
    fn podman_uses_url() {
        let command = exec_command(Kind::Podman, "/run/podman.sock", "api");
        assert!(command.starts_with("podman --url"));
    }

    #[test]
    fn quotes_are_escaped() {
        assert_eq!(quote("it's"), "'it'\\''s'");
        assert_eq!(applescript_quote("a\"b\\c"), "\"a\\\"b\\\\c\"");
    }

    #[test]
    fn bounded_terminal_helper_kills_a_hung_command_at_its_limit() {
        let started = Instant::now();
        let mut command = Command::new("sh");
        command.args(["-c", "sleep 30"]);
        let error = run_bounded(command, Duration::from_millis(150)).expect_err("must time out");
        assert_eq!(error, BoundedError::TimedOut);
        assert!(started.elapsed() < Duration::from_secs(5));

        let mut ok = Command::new("sh");
        ok.args(["-c", "exit 0"]);
        assert_eq!(
            run_bounded(ok, Duration::from_secs(5)),
            Ok((true, String::new()))
        );

        let mut bad = Command::new("sh");
        bad.args(["-c", "echo denied >&2; exit 1"]);
        assert_eq!(
            run_bounded(bad, Duration::from_secs(5)),
            Ok((false, "denied\n".to_owned()))
        );
    }
}
