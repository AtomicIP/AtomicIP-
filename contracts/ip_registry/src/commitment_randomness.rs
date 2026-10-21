//! Commitment randomness verification (#1102).
//!
//! A Pedersen commitment `secret·G + blinding_factor·H` is only hiding if its
//! blinding factor was drawn from a high-entropy source. The contract never
//! sees the blinding factor, so it cannot check that directly. What it can do
//! is inspect the committed bytes for the signatures a weak randomness source
//! leaves behind:
//!
//! * **Low diversity** — very few distinct byte values across 32 bytes. A
//!   uniformly random 32-byte string has about 28 distinct values, so fewer
//!   than [`MIN_DISTINCT_BYTES`] is a strong signal of a poor generator.
//! * **Constant or periodic output** — a repeated byte or a short repeating
//!   block, typical of a zero-filled buffer or a stuck PRNG.
//! * **Arithmetic progression** — bytes that step by a fixed delta, typical of
//!   a counter used as a nonce.
//!
//! These checks are heuristics. They never reject a commitment on their own.
//! A suspicious commitment is reported through an alert event so off-chain
//! monitoring can follow up (see `batch_verify_commitments`).

/// Minimum number of distinct byte values a commitment must contain before it
/// is considered high-entropy. Random 32-byte strings fall below this with
/// negligible probability (about 1 in 10^9).
pub const MIN_DISTINCT_BYTES: u32 = 12;

/// Longest repeating block length checked when looking for periodic output.
const MAX_PERIOD: usize = 16;

/// Pattern that makes a commitment look like it came from weak randomness.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LowEntropyPattern {
    /// Every byte is zero.
    AllZero,
    /// Every byte is the same non-zero value.
    RepeatedByte,
    /// A block of 2..=[`MAX_PERIOD`] bytes repeats across the whole value.
    Periodic,
    /// Consecutive bytes differ by the same non-zero step.
    ArithmeticProgression,
}

impl LowEntropyPattern {
    /// Stable numeric code emitted in alert events.
    pub fn alert_code(&self) -> u32 {
        match self {
            LowEntropyPattern::AllZero => 1,
            LowEntropyPattern::RepeatedByte => 2,
            LowEntropyPattern::Periodic => 3,
            LowEntropyPattern::ArithmeticProgression => 4,
        }
    }
}

/// Result of analysing one commitment's bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RandomnessAssessment {
    /// Number of distinct byte values present.
    pub distinct_bytes: u32,
    /// Length of the longest run of identical consecutive bytes.
    pub longest_run: u32,
    /// Weak-randomness pattern detected, if any.
    pub pattern: Option<LowEntropyPattern>,
}

impl RandomnessAssessment {
    /// `true` when the commitment should raise an alert.
    pub fn is_suspicious(&self) -> bool {
        self.pattern.is_some() || self.distinct_bytes < MIN_DISTINCT_BYTES
    }

    /// A 0..=100 score, where 100 means the byte diversity of random data.
    /// A detected pattern caps the score at 0.
    pub fn entropy_score(&self) -> u32 {
        if self.pattern.is_some() {
            return 0;
        }
        (self.distinct_bytes * 100 / 28).min(100)
    }
}

/// Analyse a 32-byte commitment for weak-randomness signatures (#1102).
pub fn assess(bytes: &[u8; 32]) -> RandomnessAssessment {
    let mut seen = [false; 256];
    for &b in bytes.iter() {
        seen[b as usize] = true;
    }
    let distinct_bytes = seen.iter().filter(|s| **s).count() as u32;

    let mut longest_run = 1u32;
    let mut run = 1u32;
    for i in 1..bytes.len() {
        if bytes[i] == bytes[i - 1] {
            run += 1;
            longest_run = longest_run.max(run);
        } else {
            run = 1;
        }
    }

    RandomnessAssessment {
        distinct_bytes,
        longest_run,
        pattern: detect_pattern(bytes),
    }
}

fn detect_pattern(bytes: &[u8; 32]) -> Option<LowEntropyPattern> {
    if bytes.iter().all(|&b| b == 0) {
        return Some(LowEntropyPattern::AllZero);
    }
    if bytes.iter().all(|&b| b == bytes[0]) {
        return Some(LowEntropyPattern::RepeatedByte);
    }
    for period in 2..=MAX_PERIOD {
        if (period..bytes.len()).all(|i| bytes[i] == bytes[i % period]) {
            return Some(LowEntropyPattern::Periodic);
        }
    }
    let step = bytes[1].wrapping_sub(bytes[0]);
    if (1..bytes.len()).all(|i| bytes[i].wrapping_sub(bytes[i - 1]) == step) {
        return Some(LowEntropyPattern::ArithmeticProgression);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pseudo_random() -> [u8; 32] {
        // Deterministic, well-mixed bytes (xorshift-style) for a "good" sample.
        let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
        let mut out = [0u8; 32];
        for b in out.iter_mut() {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            *b = (state & 0xff) as u8;
        }
        out
    }

    #[test]
    fn well_mixed_bytes_are_not_suspicious() {
        let report = assess(&pseudo_random());
        assert!(report.pattern.is_none());
        assert!(!report.is_suspicious());
        assert!(report.entropy_score() > 0);
    }

    #[test]
    fn all_zero_is_flagged() {
        let report = assess(&[0u8; 32]);
        assert_eq!(report.pattern, Some(LowEntropyPattern::AllZero));
        assert!(report.is_suspicious());
        assert_eq!(report.entropy_score(), 0);
        assert_eq!(report.longest_run, 32);
    }

    #[test]
    fn repeated_nonzero_byte_is_flagged() {
        let report = assess(&[0xAB; 32]);
        assert_eq!(report.pattern, Some(LowEntropyPattern::RepeatedByte));
        assert_eq!(report.distinct_bytes, 1);
    }

    #[test]
    fn short_period_is_flagged() {
        let mut bytes = [0u8; 32];
        for (i, b) in bytes.iter_mut().enumerate() {
            *b = [1u8, 2, 3, 4][i % 4];
        }
        let report = assess(&bytes);
        assert_eq!(report.pattern, Some(LowEntropyPattern::Periodic));
    }

    #[test]
    fn arithmetic_progression_is_flagged() {
        let mut bytes = [0u8; 32];
        for (i, b) in bytes.iter_mut().enumerate() {
            *b = (i as u8).wrapping_mul(7).wrapping_add(3);
        }
        let report = assess(&bytes);
        assert_eq!(
            report.pattern,
            Some(LowEntropyPattern::ArithmeticProgression)
        );
    }

    #[test]
    fn low_diversity_without_pattern_is_suspicious() {
        // Eight distinct values, arranged so no period or progression applies.
        let bytes: [u8; 32] = [
            3, 9, 3, 200, 17, 9, 77, 3, 200, 17, 77, 9, 3, 200, 17, 9, 77, 3, 200, 17, 9, 77, 3,
            200, 17, 9, 77, 3, 200, 17, 9, 77,
        ];
        let report = assess(&bytes);
        assert!(report.pattern.is_none());
        assert!(report.distinct_bytes < MIN_DISTINCT_BYTES);
        assert!(report.is_suspicious());
    }

    #[test]
    fn longest_run_counts_consecutive_identical_bytes() {
        let mut bytes = pseudo_random();
        bytes[5] = 0x11;
        bytes[6] = 0x11;
        bytes[7] = 0x11;
        assert!(assess(&bytes).longest_run >= 3);
    }

    #[test]
    fn alert_codes_are_distinct() {
        let codes = [
            LowEntropyPattern::AllZero.alert_code(),
            LowEntropyPattern::RepeatedByte.alert_code(),
            LowEntropyPattern::Periodic.alert_code(),
            LowEntropyPattern::ArithmeticProgression.alert_code(),
        ];
        for (i, a) in codes.iter().enumerate() {
            for b in &codes[i + 1..] {
                assert_ne!(a, b);
            }
        }
    }
}
