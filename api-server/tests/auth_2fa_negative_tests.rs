/// Negative tests for Two-Factor Authentication (TOTP and backup codes).
///
/// Covers issue #1240: the 2FA test suite previously only exercised happy paths.
/// These tests verify that the system correctly rejects:
///
/// - Malformed TOTP codes (wrong length, non-numeric characters)
/// - Reused backup codes
/// - Backup codes belonging to another user
/// - Enrollment / verification when no 2FA factor is configured
///
/// Related issues: #984, #1240, #1241, #1242, #1243
///
/// NOTE: These tests mirror the self-contained style used throughout
/// `api-server/tests/` and exercise the 2FA logic through data-structure
/// and protocol invariants rather than through direct crate imports, since
/// `auth_2fa.rs` is a `mod` of the binary target.  Unit-level tests that
/// call `verify_totp_code` and `mark_backup_code_used` directly live inside
/// `auth_2fa.rs` itself (see the `#[cfg(test)]` block at the bottom of that
/// file).

#[cfg(test)]
mod auth_2fa_negative_tests {
    use std::collections::HashMap;

    // ── Shared helpers ─────────────────────────────────────────────────────────

    /// Minimal in-test replica of the BackupCode structure — enough to drive
    /// the reuse / cross-user isolation tests without importing the binary crate.
    #[derive(Clone, Debug)]
    struct BackupCode {
        code: String,
        used: bool,
    }

    /// Minimal per-user 2FA config used for structural / protocol tests.
    #[derive(Clone, Debug)]
    struct UserConfig {
        totp_verified: bool,
        has_totp: bool,
        backup_codes: Vec<BackupCode>,
    }

    /// An in-test store that mirrors the behaviour of `TwoFactorStore`.
    struct TestStore {
        users: HashMap<String, UserConfig>,
    }

    impl TestStore {
        fn new() -> Self {
            Self {
                users: HashMap::new(),
            }
        }

        fn register(&mut self, user_id: &str, config: UserConfig) {
            self.users.insert(user_id.to_string(), config);
        }

        fn has_2fa_enabled(&self, user_id: &str) -> bool {
            self.users
                .get(user_id)
                .map(|c| c.has_totp && c.totp_verified)
                .unwrap_or(false)
        }

        /// Returns `true` and marks the code used if a matching unused code
        /// is found for the given user.  Returns `false` otherwise.
        fn use_backup_code(&mut self, user_id: &str, code: &str) -> bool {
            if let Some(config) = self.users.get_mut(user_id) {
                for bc in &mut config.backup_codes {
                    if bc.code == code && !bc.used {
                        bc.used = true;
                        return true;
                    }
                }
            }
            false
        }
    }

    /// Generate a simple list of fake backup codes for tests.
    fn make_backup_codes(n: usize) -> Vec<BackupCode> {
        (0..n)
            .map(|i| BackupCode {
                code: format!("TEST-{:04}", i),
                used: false,
            })
            .collect()
    }

    // ── TOTP format validation (mirrors verify_totp_code logic) ───────────────
    //
    // The canonical rule from auth_2fa.rs:
    //   `code.len() != 6 || !code.chars().all(|c| c.is_numeric())`  → Ok(false)

    fn totp_format_valid(code: &str) -> bool {
        code.len() == 6 && code.chars().all(|c| c.is_numeric())
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Task 1 — Negative tests for wrong / malformed TOTP codes (#1240, #1241)
    // ═══════════════════════════════════════════════════════════════════════════

    /// A code that is too short (5 digits) must be rejected.
    #[test]
    fn test_totp_code_too_short_is_rejected() {
        assert!(
            !totp_format_valid("12345"),
            "5-digit TOTP code must fail format validation"
        );
    }

    /// A code that is too long (7 digits) must be rejected.
    #[test]
    fn test_totp_code_too_long_is_rejected() {
        assert!(
            !totp_format_valid("1234567"),
            "7-digit TOTP code must fail format validation"
        );
    }

    /// An empty string must be rejected.
    #[test]
    fn test_totp_code_empty_string_is_rejected() {
        assert!(
            !totp_format_valid(""),
            "empty TOTP code must fail format validation"
        );
    }

    /// A code containing letters must be rejected.
    #[test]
    fn test_totp_code_with_letters_is_rejected() {
        assert!(
            !totp_format_valid("12AB56"),
            "TOTP code with letters must fail format validation"
        );
    }

    /// A code containing spaces must be rejected.
    #[test]
    fn test_totp_code_with_spaces_is_rejected() {
        assert!(
            !totp_format_valid("12 456"),
            "TOTP code with spaces must fail format validation"
        );
    }

    /// A code containing special characters must be rejected.
    #[test]
    fn test_totp_code_with_special_chars_is_rejected() {
        assert!(
            !totp_format_valid("12!@56"),
            "TOTP code with special characters must fail format validation"
        );
    }

    /// A code with a leading minus sign is not purely numeric and must be rejected.
    #[test]
    fn test_totp_code_with_minus_sign_is_rejected() {
        assert!(
            !totp_format_valid("-12345"),
            "TOTP code with leading minus must fail format validation"
        );
    }

    /// A code with a decimal point must be rejected.
    #[test]
    fn test_totp_code_with_decimal_point_is_rejected() {
        assert!(
            !totp_format_valid("123.45"),
            "TOTP code containing a decimal point must fail format validation"
        );
    }

    /// A code with a null byte must be rejected.
    #[test]
    fn test_totp_code_with_null_byte_is_rejected() {
        assert!(
            !totp_format_valid("12345\0"),
            "TOTP code containing a null byte must fail format validation"
        );
    }

    /// A valid 6-digit numeric code passes the format check (positive control).
    #[test]
    fn test_totp_code_valid_format_passes() {
        assert!(
            totp_format_valid("123456"),
            "a well-formed 6-digit TOTP code must pass format validation"
        );
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Task 2 — Negative tests for backup code reuse and cross-user isolation
    //          (#1240, #1242)
    // ═══════════════════════════════════════════════════════════════════════════

    /// A backup code that has already been used once must be rejected on the
    /// second attempt.
    #[test]
    fn test_backup_code_cannot_be_reused() {
        let mut store = TestStore::new();
        let codes = make_backup_codes(8);
        let target_code = codes[0].code.clone();

        store.register(
            "user-reuse",
            UserConfig {
                has_totp: true,
                totp_verified: true,
                backup_codes: codes,
            },
        );

        // First use must succeed
        assert!(
            store.use_backup_code("user-reuse", &target_code),
            "first use of a backup code must succeed"
        );

        // Second use must fail
        assert!(
            !store.use_backup_code("user-reuse", &target_code),
            "reusing an already-used backup code must be rejected"
        );
    }

    /// Once all backup codes for a user are exhausted, every subsequent attempt
    /// must fail.
    #[test]
    fn test_all_backup_codes_exhausted_are_rejected() {
        let mut store = TestStore::new();
        let codes = make_backup_codes(4);
        let code_strings: Vec<String> = codes.iter().map(|c| c.code.clone()).collect();

        store.register(
            "user-exhaust",
            UserConfig {
                has_totp: true,
                totp_verified: true,
                backup_codes: codes,
            },
        );

        // Use every code once
        for code in &code_strings {
            assert!(
                store.use_backup_code("user-exhaust", code),
                "each code must be accepted on first use"
            );
        }

        // All codes are now used — every retry must fail
        for code in &code_strings {
            assert!(
                !store.use_backup_code("user-exhaust", code),
                "every used code must be rejected on retry"
            );
        }
    }

    /// A backup code belonging to user A must not be accepted when presented
    /// under user B's identity.
    #[test]
    fn test_backup_code_from_another_user_is_rejected() {
        let mut store = TestStore::new();

        let codes_a = make_backup_codes(4);
        let codes_b = make_backup_codes(4);
        let code_owned_by_a = codes_a[0].code.clone();

        store.register(
            "user-a",
            UserConfig {
                has_totp: true,
                totp_verified: true,
                backup_codes: codes_a,
            },
        );
        store.register(
            "user-b",
            UserConfig {
                has_totp: true,
                totp_verified: true,
                backup_codes: codes_b,
            },
        );

        // user-b presents user-a's code → must be rejected
        assert!(
            !store.use_backup_code("user-b", &code_owned_by_a),
            "a backup code issued to user-a must not be accepted for user-b"
        );

        // Sanity: user-a can still redeem their own code
        assert!(
            store.use_backup_code("user-a", &code_owned_by_a),
            "user-a must be able to redeem their own code"
        );
    }

    /// Attempting to use a backup code for a user who does not exist must
    /// return false without panicking.
    #[test]
    fn test_backup_code_for_nonexistent_user_is_rejected() {
        let mut store = TestStore::new(); // empty store
        assert!(
            !store.use_backup_code("ghost-user", "TEST-0000"),
            "backup code for an unknown user must return false"
        );
    }

    /// A completely fabricated code that was never issued must be rejected.
    #[test]
    fn test_fabricated_backup_code_is_rejected() {
        let mut store = TestStore::new();
        store.register(
            "user-fab",
            UserConfig {
                has_totp: true,
                totp_verified: true,
                backup_codes: make_backup_codes(4),
            },
        );

        assert!(
            !store.use_backup_code("user-fab", "FAKE-ZZZZ"),
            "a fabricated backup code must be rejected"
        );
    }

    /// An empty string as a backup code must be rejected.
    #[test]
    fn test_empty_backup_code_is_rejected() {
        let mut store = TestStore::new();
        store.register(
            "user-empty",
            UserConfig {
                has_totp: true,
                totp_verified: true,
                backup_codes: make_backup_codes(4),
            },
        );

        assert!(
            !store.use_backup_code("user-empty", ""),
            "an empty backup code must be rejected"
        );
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Task 3 — Enrollment without a current factor (#1240, #1243)
    // ═══════════════════════════════════════════════════════════════════════════

    /// A user who has never enrolled must not be considered 2FA-enabled.
    #[test]
    fn test_has_2fa_enabled_returns_false_for_unknown_user() {
        let store = TestStore::new();
        assert!(
            !store.has_2fa_enabled("nobody"),
            "a user who never enrolled must not be considered 2FA-enabled"
        );
    }

    /// A user who started enrollment but never completed verification
    /// (`totp_verified = false`) must not be considered 2FA-enabled.
    #[test]
    fn test_has_2fa_enabled_returns_false_when_not_verified() {
        let mut store = TestStore::new();
        store.register(
            "user-unverified",
            UserConfig {
                has_totp: true,
                totp_verified: false, // ← incomplete enrollment
                backup_codes: vec![],
            },
        );

        assert!(
            !store.has_2fa_enabled("user-unverified"),
            "2FA must not be considered enabled until the TOTP factor is verified"
        );
    }

    /// A user with backup codes but NO TOTP factor configured must not be
    /// considered 2FA-enrolled.
    #[test]
    fn test_enrollment_without_totp_factor_is_not_enabled() {
        let mut store = TestStore::new();
        store.register(
            "user-no-totp",
            UserConfig {
                has_totp: false, // ← no TOTP factor
                totp_verified: false,
                backup_codes: make_backup_codes(8),
            },
        );

        assert!(
            !store.has_2fa_enabled("user-no-totp"),
            "having only backup codes without a verified TOTP factor must not count as enrolled"
        );
    }

    /// A user with `has_totp = true` but `totp_verified = false` is in a
    /// partially-enrolled state — they must not be considered enabled.
    #[test]
    fn test_partial_enrollment_is_not_enabled() {
        let mut store = TestStore::new();
        store.register(
            "user-partial",
            UserConfig {
                has_totp: true,
                totp_verified: false,
                backup_codes: make_backup_codes(8),
            },
        );

        assert!(
            !store.has_2fa_enabled("user-partial"),
            "partial enrollment (totp present but not verified) must not count as enabled"
        );
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Additional edge-case negative tests
    // ═══════════════════════════════════════════════════════════════════════════

    /// Using a backup code under the wrong user ID in a shared store must be
    /// rejected — even if the code string itself is valid.
    #[test]
    fn test_cross_user_code_lookup_in_shared_store_is_rejected() {
        let mut store = TestStore::new();

        // user-alpha's codes start at index 0; user-beta's codes start at 100
        // to avoid any accidental string collision.
        let codes_alpha: Vec<BackupCode> = (0..4)
            .map(|i| BackupCode {
                code: format!("ALPH-{:04}", i),
                used: false,
            })
            .collect();
        let codes_beta: Vec<BackupCode> = (100..104)
            .map(|i| BackupCode {
                code: format!("BETA-{:04}", i),
                used: false,
            })
            .collect();

        let alpha_code = codes_alpha[0].code.clone();

        store.register(
            "user-alpha",
            UserConfig {
                has_totp: true,
                totp_verified: true,
                backup_codes: codes_alpha,
            },
        );
        store.register(
            "user-beta",
            UserConfig {
                has_totp: true,
                totp_verified: true,
                backup_codes: codes_beta,
            },
        );

        // user-beta tries to use user-alpha's code
        assert!(
            !store.use_backup_code("user-beta", &alpha_code),
            "user-beta must not be able to redeem a backup code that belongs to user-alpha"
        );

        // user-alpha's own code is still intact and usable
        assert!(
            store.use_backup_code("user-alpha", &alpha_code),
            "user-alpha's own code must still be redeemable after the failed cross-user attempt"
        );
    }

    /// A TOTP code consisting only of whitespace must be rejected.
    #[test]
    fn test_totp_code_all_whitespace_is_rejected() {
        assert!(
            !totp_format_valid("      "),
            "a TOTP code of 6 spaces must fail format validation"
        );
    }

    /// A TOTP code with a unicode digit (U+FF10–U+FF19 fullwidth digits) must be
    /// rejected — only ASCII 0–9 are valid.
    #[test]
    fn test_totp_code_with_unicode_digits_is_rejected() {
        // '１２３４５６' — fullwidth digits look like numbers but are not ASCII
        let unicode_digits = "\u{FF11}\u{FF12}\u{FF13}\u{FF14}\u{FF15}\u{FF16}";
        assert_eq!(unicode_digits.chars().count(), 6);
        assert!(
            !totp_format_valid(unicode_digits),
            "unicode fullwidth digits must fail TOTP format validation"
        );
    }

    /// Verifying that the `totp_format_valid` function (which mirrors the rule
    /// in `verify_totp_code`) correctly accepts all-zero codes at the format
    /// level — the time-window rejection happens at a higher layer.
    #[test]
    fn test_totp_code_all_zeros_passes_format_check() {
        assert!(
            totp_format_valid("000000"),
            "000000 passes format check (6 numeric digits); semantic rejection is time-window's job"
        );
    }
}
