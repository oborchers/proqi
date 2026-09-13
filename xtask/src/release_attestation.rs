//! Tag-bound SPDX evidence derived from the canonical primary artifact registry.

use std::{fs, path::Path};

use serde_json::json;

pub(super) fn write_tag_sbom(
    root: &Path,
    directory: &Path,
    output: &Path,
    tag: &str,
    created: &str,
) -> Result<(), String> {
    validate_tag(tag)?;
    validate_timestamp(created)?;
    let directory = resolve(root, directory);
    let output = resolve(root, output);
    let mut packages = Vec::new();
    let mut described = Vec::new();
    for (index, name) in super::release_targets::primary_artifact_names()
        .into_iter()
        .enumerate()
    {
        let path = directory.join(&name);
        let digest = super::release::checksum(&path)?;
        let identifier = format!("SPDXRef-Package-{index}");
        described.push(identifier.clone());
        packages.push(json!({
            "name": name,
            "SPDXID": identifier,
            "versionInfo": tag.trim_start_matches('v'),
            "downloadLocation": format!("https://github.com/oborchers/proqi/releases/download/{tag}/{name}"),
            "filesAnalyzed": false,
            "checksums": [{"algorithm": "SHA256", "checksumValue": digest}],
            "licenseConcluded": "NOASSERTION",
            "licenseDeclared": "NOASSERTION",
            "copyrightText": "NOASSERTION",
        }));
    }
    let document = json!({
        "spdxVersion": "SPDX-2.3",
        "dataLicense": "CC0-1.0",
        "SPDXID": "SPDXRef-DOCUMENT",
        "name": format!("Proqi {tag} release artifacts"),
        "documentNamespace": format!("https://github.com/oborchers/proqi/releases/tag/{tag}/tag-sbom"),
        "creationInfo": {
            "created": created,
            "creators": ["Tool: proqi-xtask"]
        },
        "documentDescribes": described,
        "packages": packages,
    });
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("create {}: {error}", parent.display()))?;
    }
    fs::write(
        &output,
        format!(
            "{}\n",
            serde_json::to_string_pretty(&document)
                .map_err(|error| format!("render tag SBOM: {error}"))?
        ),
    )
    .map_err(|error| format!("write {}: {error}", output.display()))
}

fn validate_tag(tag: &str) -> Result<(), String> {
    let version = tag
        .strip_prefix('v')
        .ok_or_else(|| "tag SBOM requires a v-prefixed version".to_owned())?;
    let parsed = semver::Version::parse(version)
        .map_err(|error| format!("invalid tag SBOM version: {error}"))?;
    (tag == format!("v{}.{}.{}", parsed.major, parsed.minor, parsed.patch)
        && parsed.pre.is_empty()
        && parsed.build.is_empty())
    .then_some(())
    .ok_or_else(|| "tag SBOM requires a canonical stable version".to_owned())
}

fn validate_timestamp(value: &str) -> Result<(), String> {
    let structurally_valid = value.len() == 20
        && value.as_bytes().get(4) == Some(&b'-')
        && value.as_bytes().get(7) == Some(&b'-')
        && value.as_bytes().get(10) == Some(&b'T')
        && value.as_bytes().get(13) == Some(&b':')
        && value.as_bytes().get(16) == Some(&b':')
        && value.ends_with('Z')
        && value.bytes().enumerate().all(|(index, byte)| {
            matches!(index, 4 | 7 | 10 | 13 | 16 | 19) || byte.is_ascii_digit()
        });
    structurally_valid
        .then_some(())
        .ok_or_else(|| "tag SBOM timestamp must use YYYY-MM-DDTHH:MM:SSZ".to_owned())
}

fn resolve(root: &Path, path: &Path) -> std::path::PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde_json::Value;

    use super::write_tag_sbom;

    #[test]
    fn tag_sbom_describes_every_typed_primary_artifact_by_exact_digest() {
        let temporary = tempfile::tempdir().expect("tag evidence");
        for name in crate::release_targets::primary_artifact_names() {
            fs::write(temporary.path().join(name), b"release bytes").expect("primary artifact");
        }
        let output = temporary.path().join("tag.spdx.json");
        write_tag_sbom(
            temporary.path(),
            temporary.path(),
            &output,
            "v1.2.3",
            "2026-09-12T12:34:56Z",
        )
        .expect("write SBOM");
        let value: Value =
            serde_json::from_slice(&fs::read(output).expect("read SBOM")).expect("parse SBOM");
        assert_eq!(value["spdxVersion"], "SPDX-2.3");
        assert_eq!(
            value["packages"].as_array().map(Vec::len),
            Some(crate::release_targets::primary_artifact_names().len())
        );
        assert!(
            value["packages"]
                .as_array()
                .expect("packages")
                .iter()
                .all(|package| package["checksums"][0]["checksumValue"]
                    .as_str()
                    .is_some_and(|digest| digest.len() == 64))
        );
    }

    #[test]
    fn tag_sbom_rejects_ambiguous_identity_inputs() {
        let temporary = tempfile::tempdir().expect("tag evidence");
        assert!(
            write_tag_sbom(
                temporary.path(),
                temporary.path(),
                &temporary.path().join("tag.spdx.json"),
                "v1.2",
                "2026-09-12 12:34:56",
            )
            .is_err()
        );
    }
}
