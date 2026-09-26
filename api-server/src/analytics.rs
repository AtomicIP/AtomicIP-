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

static TOTAL_COMMITMENTS: AtomicU64 = AtomicU64::new(0);
static TAGGED_COMMITMENTS: AtomicU64 = AtomicU64::new(0);
static TAG_COUNTS: Lazy<DashMap<String, AtomicU64>> = Lazy::new(DashMap::new);
static SWAP_ANALYTICS: Lazy<DashMap<&'static str, AtomicU64>> = Lazy::new(DashMap::new);

pub fn record_commitment(tags: &[String]) {
    TOTAL_COMMITMENTS.fetch_add(1, Ordering::Relaxed);
    if !tags.is_empty() {
        TAGGED_COMMITMENTS.fetch_add(1, Ordering::Relaxed);
    }
    for tag in tags {
        TAG_COUNTS.entry(tag.clone()).or_default().fetch_add(1, Ordering::Relaxed);
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
