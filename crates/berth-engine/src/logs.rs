//! Container logs: the Docker multiplexed frame format, and the local-time
//! presentation the pane asks for.

use chrono::{DateTime, Local, TimeZone};

/// Decode Docker's 8-byte frame headers (`[stream, 0, 0, 0, size:be32]`).
///
/// A TTY container sends raw bytes instead of frames. If the first header does
/// not look like a frame, the whole answer is treated as text rather than being
/// silently dropped.
pub fn demux(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return String::new();
    }
    if !looks_framed(bytes) {
        return String::from_utf8_lossy(bytes).into_owned();
    }

    let mut out = String::new();
    let mut index = 0;
    while index + 8 <= bytes.len() {
        let stream = bytes[index];
        if !matches!(stream, 0..=2) {
            // A malformed frame: keep what we have rather than inventing text.
            break;
        }
        let size = u32::from_be_bytes([
            bytes[index + 4],
            bytes[index + 5],
            bytes[index + 6],
            bytes[index + 7],
        ]) as usize;
        index += 8;
        if size == 0 {
            continue;
        }
        if index + size > bytes.len() {
            break;
        }
        out.push_str(&String::from_utf8_lossy(&bytes[index..index + size]));
        index += size;
    }
    out
}

fn looks_framed(bytes: &[u8]) -> bool {
    if bytes.len() < 8 {
        return false;
    }
    matches!(bytes[0], 0..=2) && bytes[1] == 0 && bytes[2] == 0 && bytes[3] == 0 && {
        let size = u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]) as usize;
        size <= bytes.len() - 8
    }
}

/// Replace the RFC3339 prefix Docker adds with `timestamps=1` by a local
/// `HH:MM:SS`. Lines without a timestamp are left alone.
pub fn localize(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for line in text.split_inclusive('\n') {
        let (body, newline) = match line.strip_suffix('\n') {
            Some(body) => (body, "\n"),
            None => (line, ""),
        };
        let Some((stamp, rest)) = body.split_once(' ') else {
            out.push_str(line);
            continue;
        };
        let Ok(parsed) = DateTime::parse_from_rfc3339(stamp) else {
            out.push_str(line);
            continue;
        };
        let local = Local.from_utc_datetime(&parsed.naive_utc());
        out.push_str(&local.format("%H:%M:%S").to_string());
        out.push(' ');
        out.push_str(rest);
        out.push_str(newline);
    }
    out
}

/// Lines matching `query`, case-insensitive. An empty query returns the text.
pub fn filter_lines<'a>(text: &'a str, query: &str) -> Vec<&'a str> {
    if query.is_empty() {
        return text.lines().collect();
    }
    let query = query.to_ascii_lowercase();
    text.lines()
        .filter(|line| line.to_ascii_lowercase().contains(&query))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(stream: u8, text: &str) -> Vec<u8> {
        let mut out = vec![stream, 0, 0, 0];
        out.extend_from_slice(&(text.len() as u32).to_be_bytes());
        out.extend_from_slice(text.as_bytes());
        out
    }

    #[test]
    fn demux_joins_stdout_and_stderr_frames() {
        let mut bytes = frame(1, "hello\n");
        bytes.extend_from_slice(&frame(2, "oops\n"));
        assert_eq!(demux(&bytes), "hello\noops\n");
    }

    #[test]
    fn a_tty_stream_is_raw_text() {
        assert_eq!(demux(b"plain tty output\n"), "plain tty output\n");
    }

    #[test]
    fn a_truncated_frame_keeps_the_whole_frames_before_it() {
        let mut bytes = frame(1, "first\n");
        bytes.extend_from_slice(&[1, 0, 0, 0, 0, 0, 0, 50]);
        assert_eq!(demux(&bytes), "first\n");
    }

    #[test]
    fn localize_replaces_the_utc_prefix() {
        let text = "2026-11-01T19:55:00.123456Z /docker-entrypoint.sh: ready\n";
        let out = localize(text);
        assert!(!out.contains("2026-"));
        assert!(out.ends_with("/docker-entrypoint.sh: ready\n"));
    }

    #[test]
    fn filtering_is_case_insensitive() {
        let text = "alpha\nBeta\ngamma\n";
        assert_eq!(filter_lines(text, "beta"), vec!["Beta"]);
        assert_eq!(filter_lines(text, "").len(), 3);
    }
}
