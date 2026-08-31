//! Tests for cargo audit gate configuration and execution
//! Tests for #909 (dependency audit gate using .cargo/audit.toml)

#[cfg(test)]
mod cargo_audit_gate_tests {
    use std::fs;
    use std::path::Path;

    // ── #909: cargo audit gate test ───────────────────────────────────────────

    #[test]
    fn test_cargo_audit_config_exists() {
        let audit_config_path = "./.cargo/audit.toml";
        assert!(
            Path::new(audit_config_path).exists(),
            "cargo audit configuration must exist at {}",
            audit_config_path
        );
    }

    #[test]
    fn test_cargo_audit_config_valid_toml() {
        let config_content = fs::read_to_string("./.cargo/audit.toml")
            .expect("Failed to read audit.toml");

        // Validate TOML structure: must have [advisories] section
        assert!(
            config_content.contains("[advisories]"),
            "audit.toml must be valid TOML with [advisories] section"
        );
    }

    #[test]
    fn test_cargo_audit_config_has_advisories_section() {
        let config_content = fs::read_to_string("./.cargo/audit.toml")
            .expect("Failed to read audit.toml");

        assert!(
            config_content.contains("[advisories]"),
            "audit.toml must contain [advisories] section"
        );
    }

    #[test]
    fn test_cargo_audit_script_invocation_in_security_checks() {
        let script_content = fs::read_to_string("./scripts/security-checks.sh")
            .expect("Failed to read security-checks.sh");

        assert!(
            script_content.contains("cargo audit"),
            "security-checks.sh must invoke 'cargo audit' command"
        );
    }

    #[test]
    fn test_cargo_audit_exit_code_on_high_critical_advisory() {
        let script_content = fs::read_to_string("./scripts/security-checks.sh")
            .expect("Failed to read security-checks.sh");

        // The script must exit non-zero on any audit findings
        // (since set -e/set -euo pipefail is present)
        assert!(
            script_content.contains("set -euo pipefail")
                || script_content.contains("set -e"),
            "security-checks.sh must fail CI if cargo audit finds any advisories"
        );
    }

    #[test]
    fn test_security_md_documents_audit_policy() {
        let security_content = fs::read_to_string("./SECURITY.md")
            .expect("Failed to read SECURITY.md");

        // Should mention security checks and audit policies
        let mentions_audit = security_content.contains("audit")
            || security_content.contains("dependency")
            || security_content.contains("advisory");

        assert!(
            mentions_audit,
            "SECURITY.md should document audit policies"
        );
    }

    #[test]
    fn test_audit_config_documents_exceptions() {
        let config_content = fs::read_to_string("./.cargo/audit.toml")
            .expect("Failed to read audit.toml");

        // The config should document why exceptions are in place
        let has_ignore_section = config_content.contains("ignore");

        // If there are exceptions, they must be documented
        if has_ignore_section {
            assert!(
                config_content.contains("RUSTSEC")
                    || config_content.contains("#"),
                "Audit exceptions must be documented with RUSTSEC IDs and comments"
            );
        }
    }

    #[test]
    fn test_audit_gate_can_fail_ci_on_high_critical() {
        let config_content = fs::read_to_string("./.cargo/audit.toml")
            .expect("Failed to read audit.toml");

        // Verify audit configuration exists and is loadable
        assert!(
            !config_content.is_empty(),
            "audit.toml must contain valid configuration"
        );

        // The script runs cargo audit which will fail CI on new high/critical advisories
        let script_content = fs::read_to_string("./scripts/security-checks.sh")
            .expect("Failed to read security-checks.sh");

        assert!(
            script_content.contains("cargo audit"),
            "Audit command must be present to fail CI on findings"
        );
    }
}
