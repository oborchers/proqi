//! Native Actions expiry policy. No API client or deletion capability lives here.

use std::{fs, path::Path};
use yaml_rust2::{Yaml, YamlLoader};

pub(super) fn check(root: &Path) -> Result<Vec<String>, String> {
    let mut found = Vec::new();
    let directory = root.join(".github/workflows");
    for entry in fs::read_dir(&directory).map_err(|error| error.to_string())? {
        let path = entry.map_err(|error| error.to_string())?.path();
        if !matches!(
            path.extension().and_then(|ext| ext.to_str()),
            Some("yml" | "yaml")
        ) {
            continue;
        }
        let source = fs::read_to_string(&path).map_err(|error| error.to_string())?;
        found.extend(findings(&path.to_string_lossy(), &source)?);
    }
    Ok(found)
}

fn findings(path: &str, source: &str) -> Result<Vec<String>, String> {
    let documents = YamlLoader::load_from_str(source).map_err(|error| error.to_string())?;
    if documents.len() != 1 || documents[0]["jobs"].as_hash().is_none() {
        return Err(format!("{path}: expected one workflow with jobs"));
    }
    let mut found = Vec::new();
    visit(path, &documents[0], &mut found);
    Ok(found)
}

fn visit(path: &str, node: &Yaml, found: &mut Vec<String>) {
    if let Some(action) = node["uses"].as_str() {
        if action.starts_with("actions/upload-artifact@")
            || action.starts_with("actions/upload-pages-artifact@")
        {
            let name = node["with"]["name"].as_str().unwrap_or("github-pages");
            let expected = expected_days(path, name);
            if expected.is_none() || node["with"]["retention-days"].as_i64() != expected {
                found.push(format!(
                    "{path}: upload `{name}` requires intentional retention-days {expected:?}"
                ));
            }
        }
        if action.starts_with("docker/build-push-action@")
            && node["env"]["DOCKER_BUILD_RECORD_RETENTION_DAYS"].as_i64() != Some(7)
        {
            found.push(format!(
                "{path}: Docker build records require explicit 7-day retention"
            ));
        }
    }
    match node {
        Yaml::Hash(values) => {
            for value in values.values() {
                visit(path, value, found);
            }
        }
        Yaml::Array(values) => {
            for value in values {
                visit(path, value, found);
            }
        }
        _ => {}
    }
}

fn expected_days(path: &str, name: &str) -> Option<i64> {
    match Path::new(path).file_name()?.to_str()? {
        "release-candidate.yml" if name.starts_with("release-candidate-") => Some(30),
        "release-candidate.yml"
            if name.starts_with("candidate-")
                || matches!(name, "registry-evidence" | "release-notices") =>
        {
            Some(1)
        }
        "ci.yml" if name.starts_with("migration-failure-") || name == "proqi-docs-site" => Some(7),
        "ci.yml" if name == "ci-linux-packages" => Some(1),
        "ci-linux-image.yml" if name.starts_with("ci-linux-digest-") => Some(1),
        "docs.yml" if name == "github-pages" => Some(1),
        _ => None,
    }
}

#[cfg(test)]
#[path = "artifacts_tests.rs"]
mod tests;
