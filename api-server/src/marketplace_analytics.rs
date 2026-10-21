//! Marketplace analytics dashboard (#1096).
//!
//! Swaps are recorded when they are initiated and again when they complete. The
//! dashboard at `GET /v1/analytics/marketplace` summarises completed trades
//! (volume, price trends, buyer behaviour) and all listings (seller
//! performance). A swap's category is the first tag on its IP commitment; swaps
//! on untagged IPs are grouped under `uncategorized`.

use axum::{extract::Query, Json};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Mutex;

pub const UNCATEGORIZED: &str = "uncategorized";

/// Basis points in 100%.
const BPS: i128 = 10_000;
/// Price change, in basis points, below which a trend reads as flat (1%).
const TREND_THRESHOLD_BPS: i64 = 100;

#[derive(Debug, Clone, PartialEq, Eq)]
struct SwapRecord {
    category: String,
    seller: String,
    buyer: String,
    price: i128,
    completed_at: Option<u64>,
}

static SWAPS: Lazy<Mutex<BTreeMap<u64, SwapRecord>>> = Lazy::new(|| Mutex::new(BTreeMap::new()));

fn with_swaps<T>(f: impl FnOnce(&mut BTreeMap<u64, SwapRecord>) -> T) -> T {
    let mut guard = SWAPS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    f(&mut guard)
}

/// Record a newly initiated swap. Re-recording an existing swap ID is a no-op,
/// so retried requests cannot double-count listings.
pub fn record_initiated(swap_id: u64, ip_tags: &[String], seller: &str, buyer: &str, price: i128) {
    let category = ip_tags
        .first()
        .map(String::as_str)
        .unwrap_or(UNCATEGORIZED)
        .to_string();
    with_swaps(|swaps| {
        swaps.entry(swap_id).or_insert(SwapRecord {
            category,
            seller: seller.to_string(),
            buyer: buyer.to_string(),
            price,
            completed_at: None,
        });
    });
}

/// Mark a swap completed. Returns `false` for unknown or already-completed
/// swaps, so a replayed completion event cannot inflate volume.
pub fn record_completed(swap_id: u64, completed_at: u64) -> bool {
    with_swaps(|swaps| match swaps.get_mut(&swap_id) {
        Some(swap) if swap.completed_at.is_none() => {
            swap.completed_at = Some(completed_at);
            true
        }
        _ => false,
    })
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CategoryVolume {
    pub category: String,
    pub completed_sales: u64,
    pub total_volume: i128,
    pub average_price: i128,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PriceTrendDirection {
    Up,
    Down,
    Flat,
    InsufficientData,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PriceTrend {
    pub category: String,
    pub samples: u64,
    pub average_price: i128,
    /// Change between the earlier and later halves of completed sales, in
    /// basis points. `None` when there is too little data to compare.
    pub change_bps: Option<i64>,
    pub direction: PriceTrendDirection,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SellerPerformance {
    pub seller: String,
    pub listings: u64,
    pub completed: u64,
    pub completion_rate_bps: u64,
    pub total_volume: i128,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct BuyerBehavior {
    pub buyer: String,
    pub purchases: u64,
    pub total_spent: i128,
    pub average_price: i128,
    pub categories_explored: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct MarketplaceAnalytics {
    pub volume_by_category: Vec<CategoryVolume>,
    pub price_trends: Vec<PriceTrend>,
    pub seller_performance: Vec<SellerPerformance>,
    pub buyer_behavior: Vec<BuyerBehavior>,
}

fn average(total: i128, count: u64) -> i128 {
    if count == 0 {
        0
    } else {
        total / count as i128
    }
}

fn trend(prices: &[i128]) -> (Option<i64>, PriceTrendDirection) {
    if prices.len() < 2 {
        return (None, PriceTrendDirection::InsufficientData);
    }
    let half = prices.len() / 2;
    let earlier = average(sum(&prices[..half]), half as u64);
    let later = average(sum(&prices[half..]), (prices.len() - half) as u64);
    if earlier == 0 {
        return (None, PriceTrendDirection::InsufficientData);
    }
    let change = (later as i128 - earlier as i128) * BPS / earlier as i128;
    let change = i64::try_from(change).unwrap_or(if change > 0 { i64::MAX } else { i64::MIN });
    let direction = if change > TREND_THRESHOLD_BPS {
        PriceTrendDirection::Up
    } else if change < -TREND_THRESHOLD_BPS {
        PriceTrendDirection::Down
    } else {
        PriceTrendDirection::Flat
    };
    (Some(change), direction)
}

fn sum(prices: &[i128]) -> i128 {
    prices.iter().fold(0i128, |total, price| total.saturating_add(*price))
}

/// Summarise swaps, optionally restricted to one category.
///
/// Trade volume, price trends, and buyer behaviour use completed swaps only.
/// Seller listings count every swap the seller initiated in scope.
fn compute(swaps: &BTreeMap<u64, SwapRecord>, category: Option<&str>) -> MarketplaceAnalytics {
    let in_scope = |swap: &SwapRecord| category.map_or(true, |wanted| swap.category == wanted);

    // Completed swaps in time order (then swap ID) so trends are deterministic.
    let mut completed: Vec<(u64, u64, &SwapRecord)> = swaps
        .iter()
        .filter(|(_, swap)| in_scope(*swap))
        .filter_map(|(swap_id, swap)| swap.completed_at.map(|at| (at, *swap_id, swap)))
        .collect();
    completed.sort_by_key(|(at, swap_id, _)| (*at, *swap_id));

    let mut volume: BTreeMap<&str, (u64, i128)> = BTreeMap::new();
    let mut prices_by_category: BTreeMap<&str, Vec<i128>> = BTreeMap::new();
    let mut buyers: BTreeMap<&str, (u64, i128, BTreeSet<&str>)> = BTreeMap::new();
    for (_, _, swap) in &completed {
        let entry = volume.entry(swap.category.as_str()).or_insert((0, 0));
        entry.0 += 1;
        entry.1 = entry.1.saturating_add(swap.price);
        prices_by_category
            .entry(swap.category.as_str())
            .or_default()
            .push(swap.price);

        let buyer = buyers.entry(swap.buyer.as_str()).or_insert((0, 0, BTreeSet::new()));
        buyer.0 += 1;
        buyer.1 = buyer.1.saturating_add(swap.price);
        buyer.2.insert(swap.category.as_str());
    }

    let mut volume_by_category: Vec<CategoryVolume> = volume
        .iter()
        .map(|(category, (sales, total))| CategoryVolume {
            category: category.to_string(),
            completed_sales: *sales,
            total_volume: *total,
            average_price: average(*total, *sales),
        })
        .collect();
    volume_by_category.sort_by(|a, b| b.total_volume.cmp(&a.total_volume).then_with(|| a.category.cmp(&b.category)));

    let price_trends: Vec<PriceTrend> = prices_by_category
        .iter()
        .map(|(category, prices)| {
            let (change_bps, direction) = trend(prices);
            PriceTrend {
                category: category.to_string(),
                samples: prices.len() as u64,
                average_price: average(sum(prices), prices.len() as u64),
                change_bps,
                direction,
            }
        })
        .collect();

    let mut sellers: BTreeMap<&str, (u64, u64, i128)> = BTreeMap::new();
    for swap in swaps.values().filter(|swap| in_scope(*swap)) {
        let entry = sellers.entry(swap.seller.as_str()).or_insert((0, 0, 0));
        entry.0 += 1;
        if swap.completed_at.is_some() {
            entry.1 += 1;
            entry.2 = entry.2.saturating_add(swap.price);
        }
    }
    let mut seller_performance: Vec<SellerPerformance> = sellers
        .iter()
        .map(|(seller, (listings, completed_count, total))| SellerPerformance {
            seller: seller.to_string(),
            listings: *listings,
            completed: *completed_count,
            completion_rate_bps: (*completed_count as u128 * BPS as u128 / *listings as u128) as u64,
            total_volume: *total,
        })
        .collect();
    seller_performance.sort_by(|a, b| b.total_volume.cmp(&a.total_volume).then_with(|| a.seller.cmp(&b.seller)));

    let mut buyer_behavior: Vec<BuyerBehavior> = buyers
        .iter()
        .map(|(buyer, (purchases, spent, categories))| BuyerBehavior {
            buyer: buyer.to_string(),
            purchases: *purchases,
            total_spent: *spent,
            average_price: average(*spent, *purchases),
            categories_explored: categories.len() as u64,
        })
        .collect();
    buyer_behavior.sort_by(|a, b| b.total_spent.cmp(&a.total_spent).then_with(|| a.buyer.cmp(&b.buyer)));

    MarketplaceAnalytics {
        volume_by_category,
        price_trends,
        seller_performance,
        buyer_behavior,
    }
}

/// Current analytics over the recorded swaps.
pub fn snapshot(category: Option<&str>) -> MarketplaceAnalytics {
    with_swaps(|swaps| compute(swaps, category))
}

#[derive(Debug, Deserialize)]
pub struct MarketplaceAnalyticsQuery {
    /// Restrict every section to one category (tag), case-insensitive.
    pub category: Option<String>,
}

/// Marketplace analytics dashboard data.
pub async fn marketplace_analytics_handler(
    Query(query): Query<MarketplaceAnalyticsQuery>,
) -> Json<MarketplaceAnalytics> {
    let category = query
        .category
        .as_deref()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty());
    Json(snapshot(category.as_deref()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn swap(category: &str, seller: &str, buyer: &str, price: i128, completed_at: Option<u64>) -> SwapRecord {
        SwapRecord {
            category: category.to_string(),
            seller: seller.to_string(),
            buyer: buyer.to_string(),
            price,
            completed_at,
        }
    }

    fn store(entries: Vec<(u64, SwapRecord)>) -> BTreeMap<u64, SwapRecord> {
        entries.into_iter().collect()
    }

    #[test]
    fn volume_counts_only_completed_swaps() {
        let swaps = store(vec![
            (1, swap("art", "alice", "bob", 100, Some(10))),
            (2, swap("art", "alice", "bob", 500, None)),
        ]);
        let analytics = compute(&swaps, None);
        assert_eq!(analytics.volume_by_category.len(), 1);
        assert_eq!(analytics.volume_by_category[0].completed_sales, 1);
        assert_eq!(analytics.volume_by_category[0].total_volume, 100);
        assert_eq!(analytics.volume_by_category[0].average_price, 100);
    }

    #[test]
    fn rising_prices_trend_up() {
        let swaps = store(vec![
            (1, swap("art", "alice", "bob", 100, Some(1))),
            (2, swap("art", "alice", "bob", 150, Some(2))),
        ]);
        let trend = &compute(&swaps, None).price_trends[0];
        assert_eq!(trend.direction, PriceTrendDirection::Up);
        assert_eq!(trend.change_bps, Some(5_000));
        assert_eq!(trend.samples, 2);
    }

    #[test]
    fn falling_prices_trend_down() {
        let swaps = store(vec![
            (1, swap("art", "alice", "bob", 200, Some(1))),
            (2, swap("art", "alice", "bob", 100, Some(2))),
        ]);
        let trend = &compute(&swaps, None).price_trends[0];
        assert_eq!(trend.direction, PriceTrendDirection::Down);
        assert_eq!(trend.change_bps, Some(-5_000));
    }

    #[test]
    fn stable_prices_trend_flat() {
        let swaps = store(vec![
            (1, swap("art", "alice", "bob", 100, Some(1))),
            (2, swap("art", "alice", "bob", 100, Some(2))),
        ]);
        let trend = &compute(&swaps, None).price_trends[0];
        assert_eq!(trend.direction, PriceTrendDirection::Flat);
        assert_eq!(trend.change_bps, Some(0));
    }

    #[test]
    fn single_sale_has_insufficient_trend_data() {
        let swaps = store(vec![(1, swap("art", "alice", "bob", 100, Some(1)))]);
        let trend = &compute(&swaps, None).price_trends[0];
        assert_eq!(trend.direction, PriceTrendDirection::InsufficientData);
        assert_eq!(trend.change_bps, None);
    }

    #[test]
    fn trend_compares_earlier_and_later_halves() {
        let swaps = store(vec![
            (1, swap("art", "alice", "bob", 100, Some(1))),
            (2, swap("art", "alice", "bob", 100, Some(2))),
            (3, swap("art", "alice", "bob", 300, Some(3))),
            (4, swap("art", "alice", "bob", 300, Some(4))),
        ]);
        let trend = &compute(&swaps, None).price_trends[0];
        assert_eq!(trend.change_bps, Some(20_000));
        assert_eq!(trend.direction, PriceTrendDirection::Up);
    }

    #[test]
    fn seller_completion_rate_reflects_listings() {
        let swaps = store(vec![
            (1, swap("art", "alice", "bob", 100, Some(1))),
            (2, swap("art", "alice", "bob", 100, None)),
        ]);
        let seller = &compute(&swaps, None).seller_performance[0];
        assert_eq!(seller.listings, 2);
        assert_eq!(seller.completed, 1);
        assert_eq!(seller.completion_rate_bps, 5_000);
        assert_eq!(seller.total_volume, 100);
    }

    #[test]
    fn buyer_behavior_tracks_spend_and_categories() {
        let swaps = store(vec![
            (1, swap("art", "alice", "bob", 100, Some(1))),
            (2, swap("music", "carol", "bob", 300, Some(2))),
        ]);
        let buyer = &compute(&swaps, None).buyer_behavior[0];
        assert_eq!(buyer.buyer, "bob");
        assert_eq!(buyer.purchases, 2);
        assert_eq!(buyer.total_spent, 400);
        assert_eq!(buyer.average_price, 200);
        assert_eq!(buyer.categories_explored, 2);
    }

    #[test]
    fn category_filter_limits_every_section() {
        let swaps = store(vec![
            (1, swap("art", "alice", "bob", 100, Some(1))),
            (2, swap("music", "carol", "dave", 300, Some(2))),
        ]);
        let analytics = compute(&swaps, Some("music"));
        assert_eq!(analytics.volume_by_category.len(), 1);
        assert_eq!(analytics.volume_by_category[0].category, "music");
        assert_eq!(analytics.seller_performance.len(), 1);
        assert_eq!(analytics.seller_performance[0].seller, "carol");
        assert_eq!(analytics.buyer_behavior.len(), 1);
        assert_eq!(analytics.buyer_behavior[0].buyer, "dave");
    }

    #[test]
    fn completion_is_recorded_once() {
        let swap_id = 9_000_001;
        record_initiated(swap_id, &["Art".to_string()], "alice", "bob", 100);
        assert!(record_completed(swap_id, 42));
        assert!(!record_completed(swap_id, 43));
        assert!(!record_completed(9_000_999, 42));
    }

    #[test]
    fn untagged_ip_is_uncategorized() {
        let swap_id = 9_000_002;
        record_initiated(swap_id, &[], "alice", "bob", 100);
        record_completed(swap_id, 1);
        let analytics = snapshot(Some(UNCATEGORIZED));
        assert!(analytics
            .volume_by_category
            .iter()
            .any(|row| row.category == UNCATEGORIZED && row.completed_sales >= 1));
    }
}
