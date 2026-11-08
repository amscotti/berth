//! Pull and run form rules, kept pure so the card can show the sentence beside
//! the field before a request is sent.

use crate::image_ref;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    Reference,
    Image,
    Name,
    Ports,
    Volumes,
    Env,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    EmptyReference,
    BadReference,
    LongReference,
    EmptyImage,
    BadImage,
    LongImage,
    BadName,
    BadPort,
    BadVolume,
    BadEnv,
    LongOptions,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    pub kind: Kind,
    pub field: Field,
    pub detail: String,
}

pub fn pull(reference: &str) -> Option<Failure> {
    if reference.is_empty() {
        return Some(failure(Kind::EmptyReference, Field::Reference, ""));
    }
    if !image_ref::valid_reference(reference) {
        return Some(failure(
            Kind::BadReference,
            Field::Reference,
            image_ref::first_line(reference),
        ));
    }
    if reference.len() > 384 {
        return Some(failure(Kind::LongReference, Field::Reference, ""));
    }
    None
}

pub fn run(image: &str, name: &str, ports: &str, volumes: &str, env: &str) -> Option<Failure> {
    if image.is_empty() {
        return Some(failure(Kind::EmptyImage, Field::Image, ""));
    }
    if !image_ref::valid_reference(image) {
        return Some(failure(
            Kind::BadImage,
            Field::Image,
            image_ref::first_line(image),
        ));
    }
    if image.len() > 384 {
        return Some(failure(Kind::LongImage, Field::Image, ""));
    }
    if !name.is_empty() && !image_ref::valid_name(name) {
        return Some(failure(Kind::BadName, Field::Name, name));
    }
    if let Some(bad) = image_ref::first_invalid_port(ports) {
        return Some(failure(Kind::BadPort, Field::Ports, bad));
    }
    if let Some(bad) = image_ref::first_invalid(volumes, ':') {
        return Some(failure(Kind::BadVolume, Field::Volumes, bad));
    }
    if let Some(bad) = image_ref::first_invalid(env, '=') {
        return Some(failure(Kind::BadEnv, Field::Env, bad));
    }
    for (field, value) in [
        (Field::Ports, ports),
        (Field::Volumes, volumes),
        (Field::Env, env),
    ] {
        if value.len() > 512 {
            return Some(failure(Kind::LongOptions, field, ""));
        }
    }
    None
}

/// One sentence for the card.
pub fn describe(failure: &Failure) -> String {
    match failure.kind {
        Kind::EmptyReference => "Enter an image reference.".into(),
        Kind::BadReference => format!(
            "Reference {} has characters an engine will reject.",
            failure.detail
        ),
        Kind::LongReference => "That reference is too long.".into(),
        Kind::EmptyImage => "Enter an image to run.".into(),
        Kind::BadImage => format!(
            "Image {} has characters an engine will reject.",
            failure.detail
        ),
        Kind::LongImage => "That image reference is too long.".into(),
        Kind::BadName => "Container names are letters, digits, dot, dash and underscore.".into(),
        Kind::BadPort => format!(
            "Port {} needs host:container or ip:host:container.",
            failure.detail
        ),
        Kind::BadVolume => format!("Volume {} needs source:container.", failure.detail),
        Kind::BadEnv => format!("Environment {} needs KEY=value.", failure.detail),
        Kind::LongOptions => "That options list is too long.".into(),
    }
}

fn failure(kind: Kind, field: Field, detail: &str) -> Failure {
    Failure {
        kind,
        field,
        detail: detail.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pull_requires_a_reference() {
        assert_eq!(pull("").unwrap().kind, Kind::EmptyReference);
        assert_eq!(pull("bad ref").unwrap().field, Field::Reference);
        assert!(pull("nginx:alpine").is_none());
    }

    #[test]
    fn run_points_at_the_first_bad_field() {
        assert_eq!(run("", "", "", "", "").unwrap().field, Field::Image);
        assert_eq!(
            run("nginx", "bad name", "", "", "").unwrap().field,
            Field::Name
        );
        assert_eq!(
            run("nginx", "", "8080", "", "").unwrap().field,
            Field::Ports
        );
        assert_eq!(
            run("nginx", "", "", "data", "").unwrap().field,
            Field::Volumes
        );
        assert_eq!(run("nginx", "", "", "", "A").unwrap().field, Field::Env);
        assert!(run("nginx:alpine", "web", "8080:80", "data:/data", "A=1").is_none());
    }

    #[test]
    fn sentences_name_the_offending_entry() {
        let failure = run("nginx", "", "8080", "", "").unwrap();
        assert!(describe(&failure).contains("8080"));
        let failure = run("nginx", "", "", "", "A").unwrap();
        assert!(describe(&failure).contains("KEY=value"));
    }
}
