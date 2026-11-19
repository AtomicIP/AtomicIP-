//! Workflow hygiene checks for `.github/workflows/*.yml`.
//!
//! These tests read the workflow files as text, so they need no YAML parser.
//! They run as part of `cargo test --workspace` in CI.

use std::fs;
use std::path::PathBuf;

fn workflows_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.github/workflows")
}

fn read_workflow(name: &str) -> String {
    let path = workflows_dir().join(name);
    fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read {}: {}", path.display(), e))
}

fn all_workflow_files() -> Vec<(String, String)> {
    let mut out = Vec::new();
    for entry in fs::read_dir(workflows_dir()).expect("workflows dir must exist") {
        let path = entry.expect("readable dir entry").path();
        if path.extension().and_then(|e| e.to_str()) == Some("yml") {
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            let content = fs::read_to_string(&path).expect("readable workflow");
            out.push((name, content));
        }
    }
    out.sort();
    out
}

/// Returns true when `line` pins a first-party `actions/*` action to a major
/// version of 1, 2 or 3 (`uses: actions/cache@v3` and similar).
fn pins_outdated_actions_major(line: &str) -> bool {
    let Some(idx) = line.find("uses:") else {
        return false;
    };
    let spec = line[idx + "uses:".len()..].trim();
    if !spec.starts_with("actions/") {
        return false;
    }
    match spec.rfind("@v") {
        Some(at) => {
            let rest = &spec[at + 2..];
            matches!(rest.chars().next(), Some('1' | '2' | '3'))
                && !rest.chars().nth(1).is_some_and(|c| c.is_ascii_digit())
        }
        None => false,
    }
}

// ── Issue #1252: replace deprecated upload-artifact@v3 and cache@v3 ──────────

mod issue_1252_deprecated_actions {
    use super::*;

    #[test]
    fn sdk_regenerate_has_no_deprecated_cache_or_upload_artifact_v3() {
        let content = read_workflow("sdk-regenerate.yml");
        assert!(
            !content.contains("actions/cache@v3"),
            "sdk-regenerate.yml must not use actions/cache@v3"
        );
        assert!(
            !content.contains("actions/upload-artifact@v3"),
            "sdk-regenerate.yml must not use actions/upload-artifact@v3"
        );
    }

    #[test]
    fn sdk_regenerate_uses_current_cache_and_upload_artifact_major() {
        let content = read_workflow("sdk-regenerate.yml");
        assert!(content.contains("actions/cache@v4"));
        assert!(content.contains("actions/upload-artifact@v4"));
    }

    #[test]
    fn no_workflow_pins_a_deprecated_first_party_action_major() {
        for (name, content) in all_workflow_files() {
            for line in content.lines() {
                assert!(
                    !pins_outdated_actions_major(line),
                    "{} pins an outdated actions/* major: {}",
                    name,
                    line.trim()
                );
            }
        }
    }
}

// ── Issue #1253: upgrade codecov-action from v3 ──────────────────────────────

mod issue_1253_codecov_action {
    use super::*;

    #[test]
    fn ci_does_not_use_codecov_action_v3() {
        let content = read_workflow("ci.yml");
        assert!(
            !content.contains("codecov/codecov-action@v3"),
            "ci.yml must not use codecov/codecov-action@v3"
        );
    }

    #[test]
    fn ci_uses_codecov_action_v5_with_coverage_upload() {
        let content = read_workflow("ci.yml");
        assert!(
            content.contains("codecov/codecov-action@v5"),
            "ci.yml must pin codecov/codecov-action@v5"
        );
        assert!(content.contains("./coverage/lcov.info"));
    }
}

// ── Issue #1254: top-level permissions on every workflow ─────────────────────

mod issue_1254_workflow_permissions {
    use super::*;

    const WORKFLOWS_MISSING_PERMISSIONS: &[&str] = &[
        "blue-green-deploy.yml",
        "canary-deploy.yml",
        "ci.yml",
        "contract-upgrade-tests.yml",
        "sdk-regenerate.yml",
        "nightly-fuzz.yml",
    ];

    /// Returns the lines of the top-level `permissions:` block, if present.
    fn top_level_permissions(content: &str) -> Option<Vec<&str>> {
        let mut lines = content.lines();
        lines.find(|l| *l == "permissions:")?;
        let mut block = Vec::new();
        for line in lines {
            if line.is_empty() {
                continue;
            }
            if !line.starts_with("  ") {
                break;
            }
            block.push(line.trim());
        }
        Some(block)
    }

    #[test]
    fn every_workflow_declares_top_level_permissions() {
        for (name, content) in all_workflow_files() {
            assert!(
                top_level_permissions(&content).is_some(),
                "{} is missing a top-level permissions block",
                name
            );
        }
    }

    #[test]
    fn listed_workflows_default_to_read_only_token() {
        for name in WORKFLOWS_MISSING_PERMISSIONS {
            let content = read_workflow(name);
            let block = top_level_permissions(&content)
                .unwrap_or_else(|| panic!("{} lacks top-level permissions", name));
            assert_eq!(
                block,
                vec!["contents: read"],
                "{} must default to contents: read only",
                name
            );
        }
    }

    #[test]
    fn sdk_regenerate_grants_pull_request_write_only_on_its_job() {
        let content = read_workflow("sdk-regenerate.yml");
        let job_start = content
            .find("  regenerate-and-validate-sdk:")
            .expect("regenerate-and-validate-sdk job must exist");
        let job = &content[job_start..];
        let job = job.find("\n  validation:").map_or(job, |i| &job[..i]);
        assert!(
            job.contains("pull-requests: write"),
            "regenerate-and-validate-sdk must grant pull-requests: write for its PR comment"
        );
        let top = content[..job_start].to_string();
        assert!(
            !top.contains("pull-requests: write"),
            "pull-requests: write must not be granted at the workflow level"
        );
    }
}
