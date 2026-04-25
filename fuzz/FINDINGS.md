# Fuzz Campaign Findings Report

## Campaign Commands Reference

Run a target for one hour:

```bash
cargo fuzz run <target> -- -max_total_time=3600
```

Run with AddressSanitizer enabled:

```bash
cargo fuzz run <target> --sanitizer address -- -max_total_time=3600
```

Replace `<target>` with one of: `fuzz_verify_commitment`, `fuzz_batch_commitments`, `fuzz_reveal_partial`.

---

## fuzz_verify_commitment

| Metric              | Result                 |
| ------------------- | ---------------------- |
| Total executions    | <!-- TODO: fill in --> |
| Unique crashes      | <!-- TODO: fill in --> |
| Coverage percentage | <!-- TODO: fill in --> |

**No crashes found.**

---

## fuzz_batch_commitments

| Metric              | Result                 |
| ------------------- | ---------------------- |
| Total executions    | <!-- TODO: fill in --> |
| Unique crashes      | <!-- TODO: fill in --> |
| Coverage percentage | <!-- TODO: fill in --> |

**No crashes found.**

---

## fuzz_reveal_partial

| Metric              | Result                 |
| ------------------- | ---------------------- |
| Total executions    | <!-- TODO: fill in --> |
| Unique crashes      | <!-- TODO: fill in --> |
| Coverage percentage | <!-- TODO: fill in --> |

**No crashes found.**

---

## Crash Report Template

If a crash is discovered, record it here using the following template:

```
### Crash: <short description>

**Harness:** fuzz_verify_commitment | fuzz_batch_commitments | fuzz_reveal_partial

**Minimized Reproducer Input:**
<paste hex or base64 of the minimized corpus artifact>

**Panic Message / Error Code:**
<paste the panic message or libFuzzer error output>

**Severity:** Critical | High | Medium | Low

**Notes:**
<any additional context, e.g. which property was violated, stack trace excerpt>
```

### Severity Classification Guide

| Severity | Description                                                                                          |
| -------- | ---------------------------------------------------------------------------------------------------- |
| Critical | Commitment verification returns `true` for an incorrect secret/blinding factor (cryptographic break) |
| High     | Duplicate IP IDs returned, monotonicity violated, or storage corruption detected                     |
| Medium   | Unexpected panic in a path that should be handled gracefully (e.g., duplicate hash not caught)       |
| Low      | Assertion failure in a non-security-critical path or minor edge-case mishandling                     |
