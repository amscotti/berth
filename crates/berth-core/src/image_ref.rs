//! Image reference and form rules. These are checked before a request so a bad
//! value becomes a sentence beside the field, not a 400 from the engine.

/// A reference the engine will accept: a registry path, optional tag/digest.
pub fn valid_reference(reference: &str) -> bool {
    if reference.is_empty() || reference.len() > 512 {
        return false;
    }
    if reference.starts_with('/') || reference.ends_with('/') {
        return false;
    }
    reference
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-' | '/' | ':' | '@'))
}

/// The first line of a bad reference, for the error sentence.
pub fn first_line(text: &str) -> &str {
    text.lines().next().unwrap_or(text)
}

/// A container name: letters, digits, dot, dash, underscore.
pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_'))
}

/// The first invalid `host:container` or `ip:host:container` port entry.
/// Accepts optional `/tcp`, `/udp`, or `/sctp` suffixes and bracketed IPv6 host IPs.
pub fn first_invalid_port(ports: &str) -> Option<&str> {
    if ports.trim().is_empty() {
        return None;
    }
    for entry in ports.split(',') {
        let entry = entry.trim();
        if entry.is_empty() {
            return Some(entry);
        }
        let Some((rest, container_spec)) = entry.rsplit_once(':') else {
            return Some(entry);
        };
        let (ip, host) = match rest.rsplit_once(':') {
            None => ("", rest),
            Some((ip, host)) if valid_host_ip(ip) => (ip, host),
            Some(_) => return Some(entry),
        };
        let _ = ip;
        let container_port = match container_spec.split_once('/') {
            None => container_spec,
            Some((port, "tcp" | "udp" | "sctp")) => port,
            Some(_) => return Some(entry),
        };
        if parse_port(host).is_none() || parse_port(container_port).is_none() {
            return Some(entry);
        }
    }
    None
}

fn valid_host_ip(ip: &str) -> bool {
    !ip.is_empty()
        && (!ip.contains(':') || (ip.starts_with('[') && ip.ends_with(']') && ip.len() > 2))
}

fn parse_port(text: &str) -> Option<u16> {
    let port = text.parse::<u16>().ok()?;
    (port > 0).then_some(port)
}

/// The first comma-separated entry with the wrong `separator` structure.
/// For `'='`, accepts `KEY=value` (where `value` may itself contain `'='`).
/// For `':'`, accepts `source:target` or `source:target:options`.
pub fn first_invalid(list: &str, separator: char) -> Option<&str> {
    if list.trim().is_empty() {
        return None;
    }
    for entry in list.split(',') {
        let entry = entry.trim();
        let valid = if separator == '=' {
            entry
                .split_once('=')
                .is_some_and(|(key, value)| !key.is_empty() && !value.is_empty())
        } else {
            let mut parts = entry.split(separator);
            let (Some(first), Some(second)) = (parts.next(), parts.next()) else {
                return Some(entry);
            };
            let third_ok = match parts.next() {
                None => true,
                Some(third) => !third.is_empty() && parts.next().is_none(),
            };
            !first.is_empty() && !second.is_empty() && third_ok
        };
        if !valid {
            return Some(entry);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn references_reject_whitespace_and_empty_parts() {
        assert!(valid_reference("nginx:alpine"));
        assert!(valid_reference("ghcr.io/org/app:1.2.3"));
        assert!(valid_reference("sha256:abc"));
        assert!(!valid_reference(""));
        assert!(!valid_reference("nginx alpine"));
        assert!(!valid_reference("/nginx"));
        assert!(!valid_reference("nginx/"));
    }

    #[test]
    fn names_are_conservative() {
        assert!(valid_name("web-1"));
        assert!(valid_name("a.b_c"));
        assert!(!valid_name(""));
        assert!(!valid_name("web 1"));
        assert!(!valid_name("web/1"));
    }

    #[test]
    fn ports_accept_host_container_or_ip_host_container() {
        assert_eq!(first_invalid_port("8080:80"), None);
        assert_eq!(first_invalid_port("53:53/udp, 8080:80/tcp"), None);
        assert_eq!(first_invalid_port("[::1]:8080:80/udp"), None);
        assert_eq!(first_invalid_port("127.0.0.1:8080:80, 9090:90"), None);
        assert_eq!(first_invalid_port("8080"), Some("8080"));
        assert_eq!(first_invalid_port("8080:80/icmp"), Some("8080:80/icmp"));
        assert_eq!(first_invalid_port("0:80"), Some("0:80"));
        assert_eq!(first_invalid_port("8080:80,"), Some(""));
    }

    #[test]
    fn pairs_need_both_sides() {
        assert_eq!(first_invalid("data:/data", ':'), None);
        assert_eq!(first_invalid("/host:/data:ro", ':'), None);
        assert_eq!(first_invalid("data:", ':'), Some("data:"));
        assert_eq!(first_invalid("a:b:c:d", ':'), Some("a:b:c:d"));
        assert_eq!(first_invalid("KEY=value,A", '='), Some("A"));
        assert_eq!(first_invalid("DB_URL=postgres://u:p@h/d?ssl=1", '='), None);
    }
}
