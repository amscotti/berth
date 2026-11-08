//! The Docker `/events` interval. The endpoint streams newline-delimited JSON;
//! berth asks for a bounded interval on each refresh and merges the result into
//! a ring. Exits, OOMs and restarts are the only events worth a toast.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Event {
    #[serde(rename = "status")]
    pub status: String,
    #[serde(rename = "id")]
    pub id: String,
    #[serde(rename = "from")]
    pub from: String,
    #[serde(rename = "Type")]
    pub kind: String,
    #[serde(rename = "Action")]
    pub action: String,
    #[serde(rename = "Actor")]
    pub actor: Actor,
    #[serde(rename = "scope")]
    pub scope: String,
    #[serde(rename = "time")]
    pub time: i64,
    #[serde(rename = "timeNano")]
    pub time_nano: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Actor {
    #[serde(rename = "ID")]
    pub id: String,
    #[serde(rename = "Attributes")]
    pub attributes: BTreeMap<String, String>,
}

impl Event {
    pub fn name(&self) -> &str {
        self.actor
            .attributes
            .get("name")
            .map(String::as_str)
            .unwrap_or_else(|| {
                if self.actor.id.is_empty() {
                    &self.id
                } else {
                    &self.actor.id
                }
            })
    }

    pub fn matches(&self, query: &str) -> bool {
        if query.is_empty() {
            return true;
        }
        let query = query.to_ascii_lowercase();
        [
            self.kind.as_str(),
            self.action.as_str(),
            self.status.as_str(),
            self.name(),
            self.from.as_str(),
            self.id.as_str(),
            self.actor.id.as_str(),
        ]
        .iter()
        .any(|field| field.to_ascii_lowercase().contains(&query))
    }

    /// The sentence a toast shows for a newly observed event, or None when the
    /// event is routine (create, connect, prune) or already in the ring.
    pub fn notice(&self) -> Option<String> {
        let detail = self.actor.attributes.get("exitCode");
        match (self.kind.as_str(), self.action.as_str()) {
            ("container", "die") => Some(match detail {
                Some(code) if code != "0" => format!("exited ({code})"),
                _ => "exited".to_owned(),
            }),
            ("container", "oom") => Some("ran out of memory".to_owned()),
            ("container", "restart") => Some("restarted".to_owned()),
            ("container", "kill") => Some("was killed".to_owned()),
            _ => None,
        }
    }
}

/// Parse a stream of newline-delimited event objects. Malformed records are
/// skipped; one bad line must not blank the feed.
pub fn parse_events(bytes: &[u8]) -> Vec<Event> {
    // A streaming deserializer stops at the first bad record. Splitting on
    // newlines keeps one malformed line from blanking the feed.
    bytes
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.iter().all(u8::is_ascii_whitespace))
        .filter_map(|line| {
            let mut event = serde_json::from_slice::<Event>(line).ok()?;
            if event.time_nano == 0 && event.time > 0 {
                event.time_nano = event.time.saturating_mul(1_000_000_000);
            }
            Some(event)
        })
        .collect()
}

/// Merge a fresh interval into the ring, newest first, de-duplicated by
/// (time_nano, action, id). The ring keeps the newest `cap` entries.
pub fn merge(existing: &[Event], fresh: &[Event], cap: usize) -> Vec<Event> {
    let mut out: Vec<Event> = Vec::with_capacity(existing.len() + fresh.len());
    for event in fresh.iter().chain(existing.iter()) {
        if out.iter().any(|kept| same(kept, event)) {
            continue;
        }
        out.push(event.clone());
    }
    out.sort_by_key(|event| std::cmp::Reverse(event.time_nano));
    out.truncate(cap);
    out
}

pub fn has(events: &[Event], event: &Event) -> bool {
    events.iter().any(|kept| same(kept, event))
}

fn same(a: &Event, b: &Event) -> bool {
    a.time_nano == b.time_nano && a.action == b.action && a.id == b.id && a.actor.id == b.actor.id
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(nano: i64, action: &str, id: &str, name: &str) -> Event {
        Event {
            id: id.into(),
            kind: "container".into(),
            action: action.into(),
            time_nano: nano,
            actor: Actor {
                id: id.into(),
                attributes: [("name".to_owned(), name.to_owned())].into_iter().collect(),
            },
            ..Default::default()
        }
    }

    #[test]
    fn parses_newline_delimited_events() {
        let raw = br#"{"status":"die","id":"a","Type":"container","Action":"die","Actor":{"ID":"a","Attributes":{"name":"web","exitCode":"1"}},"time":1,"timeNano":100}
{"status":"start","id":"b","Type":"container","Action":"start","Actor":{"ID":"b","Attributes":{"name":"api"}},"time":2,"timeNano":200}
"#;
        let events = parse_events(raw);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].name(), "web");
        assert_eq!(events[1].action, "start");
    }

    #[test]
    fn malformed_records_are_skipped() {
        let raw = b"{not json}\n{\"Action\":\"die\",\"timeNano\":5}\n";
        let events = parse_events(raw);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].action, "die");
    }

    #[test]
    fn merge_dedupes_and_keeps_newest_first() {
        let old = vec![event(100, "die", "a", "web")];
        let fresh = vec![
            event(300, "start", "b", "api"),
            event(100, "die", "a", "web"),
        ];
        let merged = merge(&old, &fresh, 10);
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].action, "start");
        assert_eq!(merged[1].action, "die");
    }

    #[test]
    fn merge_caps_the_ring() {
        let fresh: Vec<_> = (0..10)
            .map(|i| event(i, "start", &format!("id{i}"), "web"))
            .collect();
        let merged = merge(&[], &fresh, 3);
        assert_eq!(merged.len(), 3);
        assert_eq!(merged[0].time_nano, 9);
    }

    #[test]
    fn notices_name_the_interesting_events() {
        let mut die = event(1, "die", "a", "web");
        die.actor.attributes.insert("exitCode".into(), "1".into());
        assert_eq!(die.notice().as_deref(), Some("exited (1)"));
        die.actor.attributes.insert("exitCode".into(), "0".into());
        assert_eq!(die.notice().as_deref(), Some("exited"));
        assert_eq!(
            event(2, "oom", "b", "api").notice().as_deref(),
            Some("ran out of memory")
        );
        assert_eq!(event(3, "create", "c", "db").notice(), None);
    }

    #[test]
    fn search_is_case_insensitive_across_the_row_fields() {
        let event = event(1, "die", "abc", "Web");
        assert!(event.matches("web"));
        assert!(event.matches("DIE"));
        assert!(event.matches("abc"));
        assert!(!event.matches("api"));
    }
}
