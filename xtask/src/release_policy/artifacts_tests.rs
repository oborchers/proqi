use super::findings;
use yaml_rust2::{Yaml, YamlLoader};

const CANDIDATE: &str = include_str!("../../../.github/workflows/release-candidate.yml");
const CI: &str = include_str!("../../../.github/workflows/ci.yml");
const IMAGE: &str = include_str!("../../../.github/workflows/ci-linux-image.yml");
const DOCS: &str = include_str!("../../../.github/workflows/docs.yml");
const RELEASE: &str = include_str!("../../../.github/workflows/release.yml");

fn workflow(source: &str) -> Yaml {
    YamlLoader::load_from_str(source)
        .expect("workflow YAML")
        .remove(0)
}

#[test]
fn artifact_retention_is_explicit_and_matches_each_lifecycle() {
    for (path, source) in [
        ("release-candidate.yml", CANDIDATE),
        ("ci.yml", CI),
        ("ci-linux-image.yml", IMAGE),
        ("docs.yml", DOCS),
    ] {
        assert!(findings(path, source).expect("policy").is_empty());
        let omitted = source
            .lines()
            .filter(|line| {
                !line.contains("retention-days:")
                    && !line.contains("DOCKER_BUILD_RECORD_RETENTION_DAYS:")
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(!findings(path, &omitted).expect("policy").is_empty());
    }
    for days in ["0", "90", "${{ inputs.days }}"] {
        let changed =
            CANDIDATE.replace("retention-days: 1\n", &format!("retention-days: {days}\n"));
        assert!(
            !findings("release-candidate.yml", &changed)
                .expect("policy")
                .is_empty()
        );
    }
    let shortened = CANDIDATE.replace("retention-days: 7", "retention-days: 1");
    assert!(
        !findings("release-candidate.yml", &shortened)
            .expect("policy")
            .is_empty()
    );
    let extended = CI.replace("retention-days: 1", "retention-days: 7");
    assert!(!findings("ci.yml", &extended).expect("policy").is_empty());
    assert!(findings("ci.yml", "jobs: [broken]").is_err());
    assert!(findings("ci.yml", "jobs: {}\n---\njobs: {}").is_err());
    let unknown = CI.replace("name: ci-linux-packages", "name: unclassified-output");
    assert!(!findings("ci.yml", &unknown).expect("policy").is_empty());
}

#[test]
fn every_upload_individually_rejects_missing_default_or_extended_retention() {
    for (path, source) in [
        ("release-candidate.yml", CANDIDATE),
        ("ci.yml", CI),
        ("ci-linux-image.yml", IMAGE),
        ("docs.yml", DOCS),
    ] {
        let lines = source.lines().collect::<Vec<_>>();
        for (index, line) in lines.iter().enumerate() {
            let key = if line.contains("retention-days:") {
                "retention-days"
            } else if line.contains("DOCKER_BUILD_RECORD_RETENTION_DAYS:") {
                "DOCKER_BUILD_RECORD_RETENTION_DAYS"
            } else {
                continue;
            };
            let indent = &line[..line.len() - line.trim_start().len()];
            for value in [None, Some("0"), Some("90"), Some("${{ inputs.days }}")] {
                let mut changed = lines.iter().map(ToString::to_string).collect::<Vec<_>>();
                changed[index] =
                    value.map_or_else(String::new, |value| format!("{indent}{key}: {value}"));
                assert!(
                    !findings(path, &changed.join("\n"))
                        .expect("policy")
                        .is_empty(),
                    "{path}:{} accepted {value:?}",
                    index + 1
                );
            }
        }
    }
}

#[test]
fn sbom_generation_does_not_upload_implicit_default_lifetime_artifacts() {
    let candidate = workflow(CANDIDATE);
    let mut generators = 0;
    for job in candidate["jobs"].as_hash().expect("jobs").values() {
        for step in job["steps"].as_vec().expect("steps") {
            if step["uses"]
                .as_str()
                .is_some_and(|action| action.starts_with("anchore/sbom-action@"))
            {
                generators += 1;
                assert_eq!(step["with"]["upload-artifact"].as_bool(), Some(false));
                assert_eq!(step["with"]["upload-release-assets"].as_bool(), Some(false));
            }
        }
    }
    assert_eq!(generators, 3);
}

fn needs(source: &Yaml, job: &str) -> Vec<String> {
    match &source["jobs"][job]["needs"] {
        Yaml::String(name) => vec![name.clone()],
        Yaml::Array(names) => names
            .iter()
            .map(|name| name.as_str().expect("job name").to_owned())
            .collect(),
        _ => Vec::new(),
    }
}

fn steps<'a>(source: &'a Yaml, job: &str) -> &'a [Yaml] {
    source["jobs"][job]["steps"].as_vec().expect("steps")
}

fn assert_same_run_downloads(job: &Yaml) {
    for step in job["steps"].as_vec().expect("steps") {
        let is_download = step["uses"]
            .as_str()
            .is_some_and(|action| action.starts_with("actions/download-artifact@"));
        if !is_download {
            continue;
        }
        for cross_run_input in ["run-id", "github-token", "repository"] {
            assert!(step["with"][cross_run_input].is_badvalue());
        }
    }
}

#[test]
fn one_day_transfers_have_only_same_run_consumers_and_bounded_jobs() {
    for source in [CANDIDATE, CI, IMAGE, DOCS] {
        let parsed = workflow(source);
        for job in parsed["jobs"].as_hash().expect("jobs").values() {
            assert!((1..=60).contains(&job["timeout-minutes"].as_i64().expect("timeout")));
            assert_same_run_downloads(job);
        }
    }
    let candidate = workflow(CANDIDATE);
    assert_eq!(needs(&candidate, "build"), ["plan", "notices"]);
    assert_eq!(
        needs(&candidate, "assemble"),
        ["plan", "build", "crate", "installer"]
    );
    let ci = workflow(CI);
    assert_eq!(needs(&ci, "debian"), ["changes", "package_linux"]);
    let image = workflow(IMAGE);
    assert_eq!(needs(&image, "publish-manifest"), ["plan", "publish"]);
    assert_eq!(needs(&workflow(DOCS), "deploy"), ["build"]);
}

#[test]
fn assembly_keeps_all_promotion_bytes_and_checks_before_final_upload() {
    let parsed = workflow(CANDIDATE);
    let assemble = steps(&parsed, "assemble");
    let downloads = assemble
        .iter()
        .filter(|step| {
            step["uses"]
                .as_str()
                .is_some_and(|action| action.starts_with("actions/download-artifact@"))
        })
        .collect::<Vec<_>>();
    assert_eq!(downloads.len(), 2);
    assert_eq!(
        downloads[0]["with"]["pattern"].as_str(),
        Some("candidate-*")
    );
    assert_eq!(downloads[0]["with"]["merge-multiple"].as_bool(), Some(true));
    assert_eq!(
        downloads[0]["with"]["path"].as_str(),
        Some("target/release-assets")
    );
    assert_eq!(
        downloads[1]["with"]["name"].as_str(),
        Some("registry-evidence")
    );
    assert_eq!(
        downloads[1]["with"]["path"].as_str(),
        Some("target/release-assets/evidence/crate")
    );
    let upload = assemble
        .iter()
        .position(|step| step["id"].as_str() == Some("upload"))
        .expect("final upload");
    let prior = assemble[..upload]
        .iter()
        .filter_map(|step| step["run"].as_str())
        .collect::<Vec<_>>()
        .join("\n");
    for check in [
        "diff -u",
        "shasum -a 256 -c",
        "verify-debian-evidence-set",
        "gh attestation verify",
        "--source-digest",
        "candidate-manifest create",
    ] {
        assert!(prior.contains(check), "missing {check}");
    }
    assert_eq!(
        assemble[upload]["with"]["path"].as_str(),
        Some("target/release-assets")
    );
    assert_eq!(assemble[upload]["with"]["retention-days"].as_i64(), Some(7));
    assert!(assemble[upload]["if"].is_badvalue());
    let release = workflow(RELEASE);
    let promote = steps(&release, "promote")
        .iter()
        .filter_map(|step| step["run"].as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(promote.contains("cargo xtask candidate-select"));
    assert!(promote.contains("actions/artifacts/${ARTIFACT_ID}/zip"));
    for intermediate in [
        "candidate-installer",
        "registry-evidence",
        "release-notices",
        "candidate-debian-",
    ] {
        assert!(!promote.contains(intermediate));
    }
}
