//! GitHub Pages workflow policy.

use yaml_rust2::{Yaml, YamlLoader};

pub(super) const PATH: &str = ".github/workflows/docs.yml";

const REQUIRED: [&str; 11] = [
    "push:",
    "branches: [main]",
    "workflow_dispatch:",
    "contents: read",
    "persist-credentials: false",
    "cargo xtask docs",
    "target/docs-site/",
    "actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v7.0.1",
    "astral-sh/setup-uv@c771a70e6277c0a99b617c7a806ffedaca235ff9 # v9.0.0",
    "actions/upload-pages-artifact@7b1f4a764d45c48632c6b24a0339c27f5614fb0b # v4.0.0",
    "actions/deploy-pages@d6db90164ac5ed86f2b6aed7e0febac5b3c0c03e # v4.0.5",
];

pub(super) fn findings(source: &str) -> Vec<String> {
    let mut found = super::missing(PATH, source, &REQUIRED);
    found.extend(structural_findings(source));
    found
}

fn structural_findings(source: &str) -> Vec<String> {
    let documents = match YamlLoader::load_from_str(source) {
        Ok(documents) if documents.len() == 1 => documents,
        Ok(_) => return vec![format!("{PATH}: workflow must contain one YAML document")],
        Err(error) => return vec![format!("{PATH}: invalid YAML: {error}")],
    };
    let root = &documents[0];
    let mut found = Vec::new();
    let allowed_root = ["name", "on", "permissions", "concurrency", "jobs"];
    if root.as_hash().is_none_or(|mapping| {
        mapping.len() != allowed_root.len()
            || allowed_root
                .iter()
                .any(|name| !mapping.contains_key(&key(name)))
    }) {
        found.push(format!(
            "{PATH}: workflow contains unexpected top-level keys"
        ));
    }

    if !exact_triggers(value(root, "on")) {
        found.push(format!(
            "{PATH}: triggers must be exactly main pushes and manual dispatch"
        ));
    }
    if !exact_string_map(value(root, "permissions"), &[("contents", "read")]) {
        found.push(format!(
            "{PATH}: top-level permissions must contain only contents: read"
        ));
    }

    let Some(jobs) = value(root, "jobs").and_then(Yaml::as_hash) else {
        found.push(format!("{PATH}: jobs must be a mapping"));
        return found;
    };
    if jobs.len() != 2 || !jobs.contains_key(&key("build")) || !jobs.contains_key(&key("deploy")) {
        found.push(format!("{PATH}: only build and deploy jobs are allowed"));
    }

    let build = jobs.get(&key("build"));
    if build.and_then(|job| value(job, "permissions")).is_some() {
        found.push(format!(
            "{PATH}: build job must inherit read-only permissions"
        ));
    }

    let deploy = jobs.get(&key("deploy"));
    if deploy
        .and_then(|job| value(job, "if"))
        .and_then(Yaml::as_str)
        != Some("github.ref == 'refs/heads/main'")
    {
        found.push(format!(
            "{PATH}: deploy condition must be exactly the main branch guard"
        ));
    }
    if !exact_string_map(
        deploy.and_then(|job| value(job, "permissions")),
        &[("pages", "write"), ("id-token", "write")],
    ) {
        found.push(format!(
            "{PATH}: deploy permissions must contain only pages: write and id-token: write"
        ));
    }
    if deploy
        .and_then(|job| value(job, "environment"))
        .and_then(|environment| value(environment, "name"))
        .and_then(Yaml::as_str)
        != Some("github-pages")
    {
        found.push(format!(
            "{PATH}: deploy environment must be named github-pages"
        ));
    }

    for (name, job) in jobs {
        check_actions(name.as_str().unwrap_or("<non-string>"), job, &mut found);
    }
    found
}

fn exact_triggers(triggers: Option<&Yaml>) -> bool {
    let Some(triggers) = triggers.and_then(Yaml::as_hash) else {
        return false;
    };
    let Some(push) = triggers.get(&key("push")) else {
        return false;
    };
    let Some(push_mapping) = push.as_hash() else {
        return false;
    };
    let branches = value(push, "branches").and_then(Yaml::as_vec);
    triggers.len() == 2
        && push_mapping.len() == 1
        && matches!(triggers.get(&key("workflow_dispatch")), Some(Yaml::Null))
        && branches.is_some_and(|values| values.len() == 1 && values[0].as_str() == Some("main"))
}

fn exact_string_map(value: Option<&Yaml>, entries: &[(&str, &str)]) -> bool {
    let Some(mapping) = value.and_then(Yaml::as_hash) else {
        return false;
    };
    mapping.len() == entries.len()
        && entries.iter().all(|(name, expected)| {
            mapping.get(&key(name)).and_then(Yaml::as_str) == Some(*expected)
        })
}

fn check_actions(job_name: &str, job: &Yaml, found: &mut Vec<String>) {
    let Some(steps) = value(job, "steps").and_then(Yaml::as_vec) else {
        found.push(format!("{PATH}: job {job_name:?} must contain steps"));
        return;
    };
    for step in steps {
        let Some(action) = value(step, "uses") else {
            continue;
        };
        let Some(action) = action.as_str() else {
            found.push(format!("{PATH}: job {job_name:?} has a non-string action"));
            continue;
        };
        let pinned = action.rsplit_once('@').is_some_and(|(_, digest)| {
            digest.len() == 40 && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
        });
        if !pinned {
            found.push(format!(
                "{PATH}: job {job_name:?} action is not pinned to a commit: {action}"
            ));
        }
        if action.starts_with("actions/checkout@")
            && value(step, "with").and_then(|options| value(options, "persist-credentials"))
                != Some(&Yaml::Boolean(false))
        {
            found.push(format!(
                "{PATH}: checkout in job {job_name:?} must disable persisted credentials"
            ));
        }
    }
}

fn value<'a>(value: &'a Yaml, name: &str) -> Option<&'a Yaml> {
    value.as_hash()?.get(&key(name))
}

fn key(name: &str) -> Yaml {
    Yaml::String(name.to_owned())
}

#[cfg(test)]
mod tests {
    use super::findings;

    #[test]
    fn workflow_is_pinned_and_deploys_main_only() {
        let source = include_str!("../../../.github/workflows/docs.yml");
        assert!(findings(source).is_empty());

        for unsafe_change in [
            source.replace("if: github.ref == 'refs/heads/main'", "if: success()"),
            source.replace(
                "if: github.ref == 'refs/heads/main'",
                "if: github.ref == 'refs/heads/main' || always()",
            ),
            source.replace(
                "actions/deploy-pages@d6db90164ac5ed86f2b6aed7e0febac5b3c0c03e",
                "actions/deploy-pages@v4",
            ),
            format!("{source}\npull_request:\n"),
            source.replace("workflow_dispatch:", "pull_request_target:"),
            source.replace("workflow_dispatch:", "schedule:\n    - cron: '0 0 * * *'"),
            source.replace(
                "    branches: [main]",
                "    branches: [main]\n    paths-ignore: ['docs/**']",
            ),
            source.replace(
                "  push:\n    branches: [main]\n  workflow_dispatch:",
                "    push:\n      branches: [main]\n    workflow_dispatch:\n    schedule:",
            ),
            source.replace("pages: write", "contents: write"),
            source.replace(
                "  build:\n    name:",
                "  build:\n    permissions: write-all\n    name:",
            ),
            source.replace(
                "  build:\n    name:",
                "  build:\n    \"permissions\": write-all\n    name:",
            ),
            source.replace(
                "    permissions:\n      pages: write",
                "    env:\n      pages: write",
            ),
            source.replace(
                "    environment:\n      name: github-pages",
                "    env:\n      name: github-pages",
            ),
            source.replace(
                "permissions:\n  contents: read",
                "permissions:\n  contents: read\n  actions: write",
            ),
            source.replace(
                "      id-token: write",
                "      id-token: write\n      actions: write",
            ),
            source.replace(
                "  build:\n    name:",
                "  build:\n    permissions:\n      actions: write\n    name:",
            ),
            source.replace(
                "  deploy:\n    name:",
                "  deploy:\n    permissions:\n      pages: write\n      id-token: write\n    name:",
            ),
            source.replace("    if: github.ref == 'refs/heads/main'\n", "")
                + "\n  build_guard:\n    if: github.ref == 'refs/heads/main'\n",
            format!(
                "{source}\n  extra:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: example/unpinned@v1\n"
            ),
            source.replace(
                "    steps:\n",
                "    steps:\n      - \"uses\": example/unpinned@v1\n",
            ),
        ] {
            assert!(
                !findings(&unsafe_change).is_empty(),
                "unsafe workflow accepted:\n{unsafe_change}"
            );
        }
    }
}
