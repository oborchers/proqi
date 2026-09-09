use super::findings;

const PROFILES: &str = "profile: [ubuntu-22.04, ubuntu-24.04, debian-bookworm]";

#[test]
fn complete_debian_profile_set_is_required() {
    let release = include_str!("../../.github/workflows/release.yml");
    let candidate = include_str!("../../.github/workflows/release-candidate.yml");
    let ci = include_str!("../../.github/workflows/ci.yml");
    let image = include_str!("../../.github/workflows/ci-linux-image.yml");
    let missing_profile = ci.replace(PROFILES, "profile: [ubuntu-22.04, ubuntu-24.04]");
    let found = findings(release, candidate, &missing_profile, image);
    assert!(found.iter().any(|item| item.contains(PROFILES)));
}
