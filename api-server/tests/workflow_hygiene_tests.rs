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
