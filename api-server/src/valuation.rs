//! IP valuation engine (#1092).
//!
//! Prices an IP commitment from comparable historical sales. Comparables are
//! other IPs that share at least one tag with the target commitment. The
//! estimate is the median comparable price, adjusted by the recent market
//! trend and discounted for price dispersion (risk).

use axum::{extract::Path, http::StatusCode, Json};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::sync::Mutex;

use crate::commitments;
use crate::schemas::ErrorResponse;

/// Maximum absolute market-trend adjustment, in basis points (±20%).
const MAX_TREND_ADJUSTMENT_BPS: i128 = 2_000;
/// Maximum risk discount, in basis points (50%).
const MAX_RISK_DISCOUNT_BPS: i128 = 5_000;
const BPS_DENOMINATOR: i128 = 10_000;

/// A completed sale of an IP, used as a comparable.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Sale {
    pub ip_id: u64,
    /// Sale price in stroops.
    pub price: i128,
    /// Unix timestamp (seconds) of the sale.
    pub timestamp: u64,
    pub tags: Vec<String>,
}

/// Risk-adjusted valuation of a single commitment.
#[derive(Debug, Clone, Serialize, PartialEq, utoipa::ToSchema)]
pub struct Valuation {
    pub ip_id: u64,
    pub comparable_count: usize,
    pub median_price: i128,
    pub trend_bps: i128,
    pub risk_discount_bps: i128,
    /// Final estimate in stroops.
    pub estimated_value: i128,
}

static SALES: Lazy<Mutex<Vec<Sale>>> = Lazy::new(|| Mutex::new(Vec::new()));

/// Record a completed sale to be used as a comparable.
pub fn record_sale(sale: Sale) -> Result<(), String> {
    if sale.price <= 0 {
        return Err("sale price must be positive".to_string());
    }
    SALES.lock().unwrap_or_else(|e| e.into_inner()).push(sale);
    Ok(())
}

/// Sales of other IPs that share at least one tag with `tags`.
pub fn comparable_sales(tags: &[String], exclude_ip: u64) -> Vec<Sale> {
    let sales = SALES.lock().unwrap_or_else(|e| e.into_inner());
    sales
        .iter()
        .filter(|sale| sale.ip_id != exclude_ip)
        .filter(|sale| sale.tags.iter().any(|tag| tags.contains(tag)))
        .cloned()
        .collect()
}

/// Median of a non-empty slice of prices. Returns `None` for an empty slice.
pub fn median(prices: &[i128]) -> Option<i128> {
    if prices.is_empty() {
        return None;
    }
    let mut sorted = prices.to_vec();
    sorted.sort_unstable();
    let mid = sorted.len() / 2;
    Some(if sorted.len() % 2 == 0 {
        (sorted[mid - 1] + sorted[mid]) / 2
    } else {
        sorted[mid]
    })
}

/// Market trend in basis points: change in average price between the older
/// and the more recent half of the sales (ordered by timestamp). Returns 0
/// when there are fewer than two sales, and is clamped to ±`MAX_TREND_ADJUSTMENT_BPS`.
pub fn trend_bps(sales: &[Sale]) -> i128 {
    if sales.len() < 2 {
        return 0;
    }
    let mut ordered: Vec<&Sale> = sales.iter().collect();
    ordered.sort_by_key(|sale| sale.timestamp);
    let mid = ordered.len() / 2;
    let avg = |slice: &[&Sale]| -> i128 {
        slice.iter().map(|s| s.price).sum::<i128>() / slice.len() as i128
    };
    let older = avg(&ordered[..mid]);
    let recent = avg(&ordered[mid..]);
    if older <= 0 {
        return 0;
    }
    let change = (recent - older) * BPS_DENOMINATOR / older;
    change.clamp(-MAX_TREND_ADJUSTMENT_BPS, MAX_TREND_ADJUSTMENT_BPS)
}

/// Risk discount in basis points, derived from price dispersion: the
/// coefficient of variation of comparable prices. Clamped to `MAX_RISK_DISCOUNT_BPS`.
pub fn risk_discount_bps(prices: &[i128]) -> i128 {
    if prices.len() < 2 {
        // A single comparable gives no evidence of dispersion; apply the
        // maximum discount because the estimate is unsupported.
        return MAX_RISK_DISCOUNT_BPS;
    }
    let n = prices.len() as i128;
    let mean = prices.iter().sum::<i128>() / n;
    if mean <= 0 {
        return MAX_RISK_DISCOUNT_BPS;
    }
    let variance = prices
        .iter()
        .map(|p| {
            let d = p - mean;
            d * d
        })
        .sum::<i128>()
        / n;
    let std_dev = isqrt(variance);
    (std_dev * BPS_DENOMINATOR / mean).min(MAX_RISK_DISCOUNT_BPS)
}

/// Build a valuation from comparable sales. Returns `None` when there are no comparables.
pub fn value_commitment(ip_id: u64, comparables: &[Sale]) -> Option<Valuation> {
    let prices: Vec<i128> = comparables.iter().map(|s| s.price).collect();
    let median_price = median(&prices)?;
    let trend = trend_bps(comparables);
    let risk = risk_discount_bps(&prices);
    let adjusted = median_price * (BPS_DENOMINATOR + trend) / BPS_DENOMINATOR;
    let estimated_value = adjusted * (BPS_DENOMINATOR - risk) / BPS_DENOMINATOR;
    Some(Valuation {
        ip_id,
        comparable_count: comparables.len(),
        median_price,
        trend_bps: trend,
        risk_discount_bps: risk,
        estimated_value,
    })
}

fn isqrt(n: i128) -> i128 {
    if n <= 0 {
        return 0;
    }
    let mut x = (n as f64).sqrt() as i128;
    while x * x > n {
        x -= 1;
    }
    while (x + 1) * (x + 1) <= n {
        x += 1;
    }
    x
}

/// `GET /v1/valuation/{commitment_id}` — risk-adjusted valuation from comparable sales.
#[utoipa::path(
    get,
    path = "/v1/valuation/{commitment_id}",
    tag = "Valuation",
    params(("commitment_id" = u64, Path, description = "IP commitment identifier")),
    responses(
        (status = 200, description = "Valuation estimate", body = Valuation),
        (status = 404, description = "No comparable sales for this commitment", body = ErrorResponse),
    )
)]
pub async fn get_valuation(Path(commitment_id): Path<u64>) -> Result<Json<Valuation>, (StatusCode, Json<ErrorResponse>)> {
    let tags = commitments::get_tags(commitment_id);
    let comparables = comparable_sales(&tags, commitment_id);
    value_commitment(commitment_id, &comparables).map(Json).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: format!("no comparable sales found for commitment {commitment_id}"),
            }),
        )
    })
}

/// `POST /v1/valuation/sales` — ingest a completed sale as a comparable.
pub async fn post_sale(Json(sale): Json<Sale>) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    record_sale(sale).map(|_| StatusCode::CREATED).map_err(|error| {
        (StatusCode::BAD_REQUEST, Json(ErrorResponse { error }))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sale(ip_id: u64, price: i128, timestamp: u64, tags: &[&str]) -> Sale {
        Sale {
            ip_id,
            price,
            timestamp,
            tags: tags.iter().map(|t| t.to_string()).collect(),
        }
    }

    #[test]
    fn median_handles_odd_even_and_empty() {
        assert_eq!(median(&[]), None);
        assert_eq!(median(&[5, 1, 3]), Some(3));
        assert_eq!(median(&[4, 1, 3, 2]), Some(2));
    }

    #[test]
    fn trend_is_positive_when_recent_sales_are_higher() {
        let sales = [sale(1, 100, 1, &["a"]), sale(2, 200, 2, &["a"])];
        // A 100% rise is clamped to the maximum adjustment.
        assert_eq!(trend_bps(&sales), MAX_TREND_ADJUSTMENT_BPS);
    }

    #[test]
    fn trend_is_zero_with_fewer_than_two_sales() {
        assert_eq!(trend_bps(&[sale(1, 100, 1, &["a"])]), 0);
    }

    #[test]
    fn identical_prices_have_no_risk_discount() {
        assert_eq!(risk_discount_bps(&[100, 100, 100]), 0);
    }

    #[test]
    fn single_comparable_gets_maximum_risk_discount() {
        assert_eq!(risk_discount_bps(&[100]), MAX_RISK_DISCOUNT_BPS);
    }

    #[test]
    fn valuation_applies_trend_and_risk_discount() {
        let comparables = [
            sale(1, 1_000, 1, &["a"]),
            sale(2, 1_000, 2, &["a"]),
            sale(3, 1_000, 3, &["a"]),
            sale(4, 1_000, 4, &["a"]),
        ];
        let v = value_commitment(99, &comparables).unwrap();
        assert_eq!(v.comparable_count, 4);
        assert_eq!(v.median_price, 1_000);
        assert_eq!(v.trend_bps, 0);
        assert_eq!(v.risk_discount_bps, 0);
        assert_eq!(v.estimated_value, 1_000);
    }

    #[test]
    fn valuation_is_none_without_comparables() {
        assert!(value_commitment(1, &[]).is_none());
    }

    #[test]
    fn comparables_match_on_shared_tag_and_exclude_self() {
        record_sale(sale(7001, 500, 1, &["valuation-test-tag"])).unwrap();
        record_sale(sale(7002, 700, 2, &["unrelated-tag"])).unwrap();
        let tags = vec!["valuation-test-tag".to_string()];
        let found = comparable_sales(&tags, 7000);
        assert!(found.iter().any(|s| s.ip_id == 7001));
        assert!(found.iter().all(|s| s.ip_id != 7002));
        assert!(comparable_sales(&tags, 7001).iter().all(|s| s.ip_id != 7001));
    }

    #[test]
    fn rejects_non_positive_sale_price() {
        assert!(record_sale(sale(1, 0, 1, &["a"])).is_err());
    }
}
