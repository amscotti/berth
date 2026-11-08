//! Opening a container shell in the user's terminal, pinned to the socket the
//! window is reading.

use std::process::Command;

use berth_engine::discovery::Kind;

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
        let output = Command::new("osascript")
            .arg("-e")
            .arg("tell application \"Terminal\" to activate")
            .arg("-e")
            .arg(script)
            .output()
            .map_err(|error| format!("could not open Terminal: {error}"))?;
        if !output.status.success() {
            let text = String::from_utf8_lossy(&output.stderr);
            return Err(format!(
                "could not open Terminal: {}",
                text.lines().next().unwrap_or("osascript failed")
            ));
        }
        return Ok(());
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
}
