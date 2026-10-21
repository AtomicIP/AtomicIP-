use dashmap::DashMap;
use once_cell::sync::Lazy;
use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Serialize)]
pub struct CommitmentAnalytics {
    pub total_commitments: u64,
    pub tagged_commitments: u64,
    pub tags: Vec<TagCount>,
    pub swaps: SwapAnalytics,
}

#[derive(Debug, Serialize)]
pub struct TagCount {
    pub tag: String,
    pub count: u64,
}

#[derive(Debug, Default, Serialize)]
pub struct SwapAnalytics {
    pub initiated: u64,
    pub pending: u64,
    pub accepted: u64,
    pub completed: u64,
    pub disputed: u64,
    pub cancelled: u64,
}

/// Maximum number of distinct tags tracked individually. Any tag beyond this
/// limit is aggregated into the `OTHER_TAG` bucket to keep cardinality bounded.
pub const MAX_DISTINCT_TAGS: usize = 100;
/// Maximum allowed length (in bytes) for a single tag.
pub const MAX_TAG_LENGTH: usize = 64;
/// Bucket used for tags that exceed the distinct-tag limit.
pub const OTHER_TAG: &str = "other";

static TOTAL_COMMITMENTS: AtomicU64 = AtomicU64::new(0);
static TAGGED_COMMITMENTS: AtomicU64 = AtomicU64::new(0);
static TAG_COUNTS: Lazy<DashMap<String, AtomicU64>> = Lazy::new(DashMap::new);
static SWAP_ANALYTICS: Lazy<DashMap<&'static str, AtomicU64>> = Lazy::new(DashMap::new);

/// Validate a user-supplied tag against the length and character policy.
///
/// Returns `Some(normalized)` when the tag is acceptable, or `None` when it
/// should be rejected. Accepted tags are lowercased and trimmed; only ASCII
/// alphanumerics, `-`, `_`, `.` and `:` are permitted.
pub fn validate_tag(tag: &str) -> Option<String> {
    let normalized = tag.trim().to_ascii_lowercase();
    if normalized.is_empty() || normalized.len() > MAX_TAG_LENGTH {
        return None;
    }
    if !normalized
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ':'))
    {
        return None;
    }
    Some(normalized)
}

fn increment_tag(tag: &str) {
    TAG_COUNTS.entry(tag.to_string()).or_default().fetch_add(1, Ordering::Relaxed);
}

pub fn record_commitment(tags: &[String]) {
    TOTAL_COMMITMENTS.fetch_add(1, Ordering::Relaxed);
    if !tags.is_empty() {
        TAGGED_COMMITMENTS.fetch_add(1, Ordering::Relaxed);
    }
    for tag in tags {
        let Some(normalized) = validate_tag(tag) else {
            continue;
        };
        // Fast path: tag already tracked.
        if TAG_COUNTS.contains_key(&normalized) {
            increment_tag(&normalized);
            continue;
        }
        // Reserve a slot for a new distinct tag, or fall back to the "other" bucket.
        if TAG_COUNTS.len() < MAX_DISTINCT_TAGS {
            increment_tag(&normalized);
        } else {
            increment_tag(OTHER_TAG);
        }
    }
}

pub fn record_swap_initiated() {
    SWAP_ANALYTICS.entry("initiated").or_default().fetch_add(1, Ordering::Relaxed);
    record_swap_status("Pending");
}

pub fn record_swap_status(status: &'static str) {
    SWAP_ANALYTICS.entry(status).or_default().fetch_add(1, Ordering::Relaxed);
}

pub fn snapshot() -> CommitmentAnalytics {
    let mut tags: Vec<TagCount> = TAG_COUNTS.iter().map(|entry| TagCount {
        tag: entry.key().clone(),
        count: entry.value().load(Ordering::Relaxed),
    }).collect();
    tags.sort_by(|left, right| right.count.cmp(&left.count).then_with(|| left.tag.cmp(&right.tag)));
    let count = |name: &'static str| SWAP_ANALYTICS.get(name).map(|v| v.load(Ordering::Relaxed)).unwrap_or(0);
    CommitmentAnalytics {
        total_commitments: TOTAL_COMMITMENTS.load(Ordering::Relaxed),
        tagged_commitments: TAGGED_COMMITMENTS.load(Ordering::Relaxed),
        tags,
        swaps: SwapAnalytics {
            initiated: count("initiated"),
            pending: count("Pending"),
            accepted: count("Accepted"),
            completed: count("Completed"),
            disputed: count("Disputed"),
            cancelled: count("Cancelled"),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_tag_enforces_policy() {
        assert_eq!(validate_tag("  Alpha "), Some("alpha".to_string()));
        assert_eq!(validate_tag("swap-1_a.b:c"), Some("swap-1_a.b:c".to_string()));
        assert_eq!(validate_tag(""), None);
        assert_eq!(validate_tag("   "), None);
        assert_eq!(validate_tag("bad tag"), None);
        assert_eq!(validate_tag("bad<script>"), None);
        assert_eq!(validate_tag(&"x".repeat(MAX_TAG_LENGTH + 1)), None);
    }

    #[test]
    fn overflow_tags_are_bucketed_as_other() {
        // Use a unique prefix so this test is isolated from other tests sharing
        // the global TAG_COUNTS map.
        let prefix = "overflowtest";
        let total = MAX_DISTINCT_TAGS + 25;
        let tags: Vec<String> = (0..total).map(|i| format!("{prefix}-{i}")).collect();

        record_commitment(&tags);

        // The map must stay bounded by the limit (plus the "other" bucket).
        assert!(TAG_COUNTS.len() <= MAX_DISTINCT_TAGS + 1);

        let other = TAG_COUNTS
            .get(OTHER_TAG)
            .map(|v| v.load(Ordering::Relaxed))
            .unwrap_or(0);
        assert!(other >= 25, "expected overflow tags in 'other', got {other}");

        // No overflow tag should have created its own series.
        let tracked = TAG_COUNTS
            .iter()
            .filter(|e| e.key().starts_with(prefix))
            .count();
        assert!(tracked <= MAX_DISTINCT_TAGS);
    }
}
