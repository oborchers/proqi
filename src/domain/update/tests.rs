use std::str::FromStr as _;

use super::{
    EXTERNAL_RESTART_MAX_EXPECTATIONS, ExternalRestartExpectation, ExternalRestartPending,
    InstallationIdentity, InstalledVersionRelation, StableVersion, UpdateCacheState,
};
use crate::domain::{InstanceId, RequestId, SessionId};

#[test]
fn stable_versions_are_canonical_and_ordered() {
    let old = StableVersion::parse("0.9.9").expect("old version");
    let new = StableVersion::parse_tag("v0.10.0").expect("new version");
    assert!(new > old);
    assert_eq!(new.to_string(), "0.10.0");
    assert_eq!(new.tag(), "v0.10.0");
    assert_eq!(
        new.relation_to_observed(&old),
        InstalledVersionRelation::CurrentNewer
    );
    assert_eq!(
        old.relation_to_observed(&new),
        InstalledVersionRelation::CurrentOlder
    );
    assert_eq!(
        new.relation_to_observed(&new),
        InstalledVersionRelation::Equal
    );
    for invalid in ["v0.1.0", "0.1", "01.2.3", "0.1.0-alpha.1", "0.1.0+build"] {
        assert!(StableVersion::parse(invalid).is_err(), "accepted {invalid}");
    }
    assert!(StableVersion::parse_tag("0.1.0").is_err());
}

#[test]
fn installation_identity_preserves_all_digest_bytes() {
    let digest = std::array::from_fn(|index| u8::try_from(index).expect("byte index"));
    let identity = InstallationIdentity::from_digest(digest);
    let encoded = identity.to_string();
    assert_eq!(encoded.len(), 64);
    assert_eq!(InstallationIdentity::from_str(&encoded), Ok(identity));
    assert_eq!(identity.as_bytes(), digest);
    assert!(InstallationIdentity::from_str(&encoded.to_uppercase()).is_err());
    assert!(InstallationIdentity::from_str(&encoded[..62]).is_err());
}

#[test]
fn legacy_update_cache_defaults_the_refresh_generation() {
    let state: UpdateCacheState = serde_json::from_str("{}").expect("legacy cache");
    assert_eq!(state.refresh_generation, 0);
}

#[test]
fn external_restart_authority_is_typed_bounded_and_round_trips() {
    let request = RequestId::from_str("req_06g30t8fudrq55fdkjqr6mpe44").expect("request ID");
    let session = SessionId::from_str("ses_06g30t7dv5qv55n1ppn3clis3k").expect("session ID");
    let instance = InstanceId::from_str("ins_06g3cnfeelq3707alnmfsvn1vo").expect("instance ID");
    let target = StableVersion::parse("0.10.0").expect("target");
    let expectation = ExternalRestartExpectation::new(
        session,
        instance,
        42,
        StableVersion::parse("0.9.0").expect("previous"),
    );
    let pending = ExternalRestartPending::new(target.clone(), request, vec![expectation.clone()])
        .expect("valid authority");
    let encoded = serde_json::to_vec(&pending).expect("encoded authority");
    assert_eq!(
        serde_json::from_slice::<ExternalRestartPending>(&encoded).expect("decoded authority"),
        pending
    );
    assert!(
        ExternalRestartPending::new(target.clone(), request, Vec::new()).is_err(),
        "an empty cohort has no replacement authority"
    );
    assert!(
        ExternalRestartPending::new(
            target.clone(),
            request,
            vec![ExternalRestartExpectation::new(
                session,
                instance,
                0,
                StableVersion::parse("0.9.0").expect("previous"),
            )],
        )
        .is_err(),
        "a zero PID cannot prove same-process replacement"
    );
    assert!(
        ExternalRestartPending::new(
            target,
            request,
            vec![expectation; EXTERNAL_RESTART_MAX_EXPECTATIONS + 1],
        )
        .is_err(),
        "the durable cohort must remain bounded"
    );
}
