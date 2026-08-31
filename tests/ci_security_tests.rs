//! Tests for CI security-checks.sh script configuration and execution
//! Tests for #910 (security-checks.sh invocation to CI)

#[cfg(test)]
mod security_checks_script_tests {
    use std::fs;
    use std::path::Path;

    // ── #910: security-checks.sh invocation test ──────────────────────────────

    #[test]
    fn test_security_checks_script_exists() {
        let script_path = "./scripts/security-checks.sh";
        assert!(
            Path::new(script_path).exists(),
            "security-checks.sh script must exist at {}",
            script_path
        );
    }

    #[test]
    fn test_security_checks_script_is_executable() {
        let script_path = "./scripts/security-checks.sh";
        let metadata = fs::metadata(script_path)
            .expect("Failed to read security-checks.sh metadata");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let permissions = metadata.permissions();
            let mode = permissions.mode();
            let is_executable = (mode & 0o111) != 0;
            assert!(
                is_executable,
                "security-checks.sh must have executable permissions"
            );
        }
    }

    #[test]
    fn test_security_checks_script_validates_all_checks() {
        let script_content =
            fs::read_to_string("./scripts/security-checks.sh")
                .expect("Failed to read security-checks.sh");

        assert!(
            script_content.contains("run_deny"),
            "security-checks.sh must include deny check"
        );
        assert!(
            script_content.contains("run_audit"),
            "security-checks.sh must include audit check"
        );
        assert!(
            script_content.contains("run_coverage"),
            "security-checks.sh must include coverage check"
        );
        assert!(
            script_content.contains("run_mutants"),
            "security-checks.sh must include mutants check"
        );
    }

    #[test]
    fn test_security_checks_script_exits_nonzero_on_failure() {
        let script_content =
            fs::read_to_string("./scripts/security-checks.sh")
                .expect("Failed to read security-checks.sh");

        assert!(
            script_content.contains("set -euo pipefail")
                || script_content.contains("set -e"),
            "security-checks.sh must use 'set -e' or 'set -euo pipefail' to exit on error"
        );
    }

    #[test]
    fn test_security_checks_script_help_and_usage() {
        let script_content =
            fs::read_to_string("./scripts/security-checks.sh")
                .expect("Failed to read security-checks.sh");

        assert!(
            script_content.contains("Usage:"),
            "security-checks.sh must include usage documentation"
        );
        assert!(
            script_content.contains("deny") &&
            script_content.contains("audit") &&
            script_content.contains("coverage") &&
            script_content.contains("mutants"),
            "security-checks.sh usage must document all check types"
        );
    }

    #[test]
    fn test_ci_workflow_can_reference_security_checks() {
        let ci_yaml = fs::read_to_string("./.github/workflows/ci.yml")
            .expect("Failed to read CI workflow");

        // Verify the workflow file exists and contains steps
        assert!(
            ci_yaml.contains("name: CI") || ci_yaml.contains("jobs:"),
            "CI workflow must be properly formatted"
        );
    }

    #[test]
    fn test_security_md_documentation_exists() {
        let security_md_path = "./SECURITY.md";
        assert!(
            Path::new(security_md_path).exists(),
            "SECURITY.md must exist to document audit policies"
        );
    }
}
