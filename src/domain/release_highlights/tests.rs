use super::{
    ReleaseHighlightAnnouncement, ReleaseHighlightAnnouncementError, ReleaseHighlightGroup,
    ReleaseHighlightsError, ReleaseHighlightsManifest,
};
use std::str::FromStr as _;

use crate::domain::{SessionId, StableVersion};

fn session() -> SessionId {
    SessionId::from_str("ses_06g30t7dv5qv55n1ppn3clis3k").expect("canonical session")
}

fn manifest(groups: &str) -> String {
    format!(r#"{{"schema_version":1,"releases":[{groups}]}}"#)
}

fn group(version: &str, highlights: &[&str]) -> String {
    format!(
        r#"{{"version":"{version}","highlights":{}}}"#,
        serde_json::to_string(highlights).expect("fixture JSON")
    )
}

#[test]
fn packaged_manifest_is_valid_and_ends_at_the_cargo_version() {
    let parsed =
        ReleaseHighlightsManifest::parse_json(include_str!("../../../release-highlights.json"))
            .expect("packaged highlights");
    let installed = StableVersion::parse(env!("CARGO_PKG_VERSION")).expect("Cargo version");
    assert_eq!(
        parsed.releases().last().map(ReleaseHighlightGroup::version),
        Some(&installed)
    );
}

#[test]
fn skipped_versions_are_grouped_through_the_exact_target() {
    let parsed =
        ReleaseHighlightsManifest::parse_json(include_str!("../../../release-highlights.json"))
            .expect("packaged highlights");
    let groups = parsed
        .between(
            &StableVersion::parse("0.1.2").expect("previous"),
            &StableVersion::parse("0.4.0").expect("target"),
        )
        .expect("skipped releases");
    assert_eq!(
        groups
            .iter()
            .map(|group| group.version().to_string())
            .collect::<Vec<_>>(),
        ["0.2.0", "0.3.0", "0.4.0"]
    );
    assert!(
        parsed
            .between(
                &StableVersion::parse("0.4.0").expect("current"),
                &StableVersion::parse("0.4.0").expect("same"),
            )
            .is_none()
    );
    assert!(
        parsed
            .between(
                &StableVersion::parse("0.3.0").expect("previous"),
                &StableVersion::parse("9.9.9").expect("missing"),
            )
            .is_none()
    );
}

#[test]
fn schema_versions_order_counts_and_text_are_strict() {
    let valid = group("1.2.3", &["One", "Two", "Three"]);
    assert!(ReleaseHighlightsManifest::parse_json(&manifest(&valid)).is_ok());

    let malformed = [
        (
            r#"{"schema_version":2,"releases":[]}"#.to_owned(),
            ReleaseHighlightsError::UnsupportedSchema,
        ),
        (
            manifest(&group("1.2", &["One", "Two", "Three"])),
            ReleaseHighlightsError::InvalidVersion,
        ),
        (
            manifest(&group("1.2.3-alpha.1", &["One", "Two", "Three"])),
            ReleaseHighlightsError::InvalidVersion,
        ),
        (
            manifest(&group("1.2.3", &["One", "Two"])),
            ReleaseHighlightsError::InvalidItemCount,
        ),
        (
            manifest(&group(
                "1.2.3",
                &["One", "Two", "Three", "Four", "Five", "Six", "Seven"],
            )),
            ReleaseHighlightsError::InvalidItemCount,
        ),
        (
            manifest(&group("1.2.3", &["One", "Two", " Two"])),
            ReleaseHighlightsError::InvalidHighlight,
        ),
        (
            manifest(&group("1.2.3", &["One", "Two", "Two"])),
            ReleaseHighlightsError::InvalidHighlight,
        ),
        (
            manifest(&group("1.2.3", &["One", "Two", "Three\nFour"])),
            ReleaseHighlightsError::InvalidHighlight,
        ),
    ];
    for (input, expected) in malformed {
        assert_eq!(ReleaseHighlightsManifest::parse_json(&input), Err(expected));
    }

    let duplicate = manifest(&format!("{valid},{valid}"));
    assert_eq!(
        ReleaseHighlightsManifest::parse_json(&duplicate),
        Err(ReleaseHighlightsError::UnorderedVersions)
    );
}

#[test]
fn unknown_fields_and_oversized_input_fail_closed() {
    assert_eq!(
        ReleaseHighlightsManifest::parse_json(r#"{"schema_version":1,"releases":[],"extra":true}"#),
        Err(ReleaseHighlightsError::Malformed)
    );
    assert_eq!(
        ReleaseHighlightsManifest::parse_json(&"x".repeat(64 * 1024 + 1)),
        Err(ReleaseHighlightsError::TooLarge)
    );
}

#[test]
fn durable_announcement_requires_an_exact_advancing_version_range() {
    let session = session();
    let previous = StableVersion::parse("1.2.2").expect("previous");
    let target = StableVersion::parse("1.2.3").expect("target");
    let mut announcement =
        ReleaseHighlightAnnouncement::pending(session, previous.clone(), target.clone())
            .expect("announcement");
    assert!(announcement.is_pending_for(session, &target));
    announcement.acknowledge();
    assert!(!announcement.is_pending_for(session, &target));
    assert_eq!(
        ReleaseHighlightAnnouncement::pending(session, target, previous),
        Err(ReleaseHighlightAnnouncementError::InvalidVersionRange)
    );
}

#[test]
fn malformed_durable_announcement_fails_deserialization() {
    let session = session();
    let value = serde_json::json!({
        "session_id": session,
        "previous_version": "2.0.0",
        "target_version": "1.0.0",
        "acknowledged": false
    });
    assert!(serde_json::from_value::<ReleaseHighlightAnnouncement>(value).is_err());
}
