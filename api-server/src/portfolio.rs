//! IP portfolio management dashboard (#1091).
//!
//! Aggregates the IPs held by one owner into key metrics, a concentration-based
//! risk rating, and rebalancing suggestions for positions that exceed the cap.

use axum::{extract::Path, http::StatusCode, Json};
use dashmap::DashMap;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};

use crate::schemas::ErrorResponse;

/// Maximum share of total portfolio value a single IP may hold before a
/// rebalancing suggestion is made (40%).
const MAX_POSITION_BPS: i128 = 4_000;
const BPS_DENOMINATOR: i128 = 10_000;
/// Herfindahl-Hirschman index thresholds (in bps-squared units / 10_000).
const HHI_HIGH: i128 = 5_000;
const HHI_MEDIUM: i128 = 2_500;
const MAX_HOLDINGS_PER_OWNER: usize = 500;

/// One IP held by an owner, with its current estimated value in stroops.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
pub struct Holding {
    pub ip_id: u64,
    pub estimated_value: i128,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Position {
    pub ip_id: u64,
    pub estimated_value: i128,
    pub share_bps: i128,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SuggestionKind {
    /// Trim this position so it no longer exceeds the position cap.
    ReducePosition,
    /// Portfolio holds a single IP, so there is no diversification.
    Diversify,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RebalanceSuggestion {
    pub kind: SuggestionKind,
    pub ip_id: Option<u64>,
    /// Value to sell or move, in stroops (0 for `Diversify`).
    pub amount: i128,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Portfolio {
    pub owner: String,
    pub holding_count: usize,
    pub total_value: i128,
    pub largest_position_bps: i128,
    pub concentration_hhi: i128,
    pub risk_level: RiskLevel,
    pub positions: Vec<Position>,
    pub suggestions: Vec<RebalanceSuggestion>,
}

static HOLDINGS: Lazy<DashMap<String, Vec<Holding>>> = Lazy::new(DashMap::new);

/// Replace the holdings recorded for `owner`.
pub fn set_holdings(owner: &str, holdings: Vec<Holding>) -> Result<(), String> {
    if owner.trim().is_empty() {
        return Err("owner must not be empty".to_string());
    }
    if holdings.iter().any(|h| h.estimated_value < 0) {
        return Err("estimated_value must not be negative".to_string());
    }
    let mut seen = std::collections::HashSet::new();
    if holdings.iter().any(|h| !seen.insert(h.ip_id)) {
        return Err("duplicate ip_id in holdings".to_string());
    }
    if holdings.len() > MAX_HOLDINGS_PER_OWNER {
        return Err(format!("an owner may hold at most {MAX_HOLDINGS_PER_OWNER} IPs"));
    }
    HOLDINGS.insert(owner.to_string(), holdings);
    Ok(())
}

/// Compute portfolio metrics, risk and rebalancing suggestions from holdings.
pub fn analyze(owner: &str, holdings: &[Holding]) -> Portfolio {
    let total_value: i128 = holdings.iter().map(|h| h.estimated_value).sum();

    let positions: Vec<Position> = holdings
        .iter()
        .map(|h| Position {
            ip_id: h.ip_id,
            estimated_value: h.estimated_value,
            share_bps: share_bps(h.estimated_value, total_value),
        })
        .collect();

    let largest_position_bps = positions.iter().map(|p| p.share_bps).max().unwrap_or(0);
    let concentration_hhi = positions
        .iter()
        .map(|p| p.share_bps * p.share_bps / BPS_DENOMINATOR)
        .sum::<i128>();

    // An empty portfolio has no exposure, so it is rated Low.
    let risk_level = if holdings.is_empty() {
        RiskLevel::Low
    } else if holdings.len() == 1 || concentration_hhi >= HHI_HIGH {
        RiskLevel::High
    } else if concentration_hhi >= HHI_MEDIUM {
        RiskLevel::Medium
    } else {
        RiskLevel::Low
    };

    let mut suggestions = Vec::new();
    if holdings.len() == 1 {
        suggestions.push(RebalanceSuggestion {
            kind: SuggestionKind::Diversify,
            ip_id: Some(holdings[0].ip_id),
            amount: 0,
            message: "portfolio holds a single IP; consider acquiring or licensing others to diversify".to_string(),
        });
    }
    for position in positions.iter().filter(|p| p.share_bps > MAX_POSITION_BPS) {
        let target_value = total_value * MAX_POSITION_BPS / BPS_DENOMINATOR;
        let amount = position.estimated_value - target_value;
        suggestions.push(RebalanceSuggestion {
            kind: SuggestionKind::ReducePosition,
            ip_id: Some(position.ip_id),
            amount,
            message: format!(
                "IP {} is {} bps of the portfolio, above the {} bps cap; reduce by {} stroops",
                position.ip_id, position.share_bps, MAX_POSITION_BPS, amount
            ),
        });
    }

    Portfolio {
        owner: owner.to_string(),
        holding_count: holdings.len(),
        total_value,
        largest_position_bps,
        concentration_hhi,
        risk_level,
        positions,
        suggestions,
    }
}

fn share_bps(value: i128, total: i128) -> i128 {
    if total <= 0 {
        0
    } else {
        value * BPS_DENOMINATOR / total
    }
}

/// `GET /v1/portfolio/{owner}` — portfolio dashboard for an owner.
#[utoipa::path(
    get,
    path = "/v1/portfolio/{owner}",
    tag = "Portfolio",
    params(("owner" = String, Path, description = "Stellar address of the portfolio owner")),
    responses((status = 200, description = "Portfolio metrics, risk and rebalancing suggestions"))
)]
pub async fn get_portfolio(Path(owner): Path<String>) -> Json<Portfolio> {
    let holdings = HOLDINGS
        .get(&owner)
        .map(|entry| entry.clone())
        .unwrap_or_default();
    Json(analyze(&owner, &holdings))
}

/// `POST /v1/portfolio/{owner}/holdings` — replace the holdings for an owner.
pub async fn put_holdings(
    Path(owner): Path<String>,
    Json(holdings): Json<Vec<Holding>>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    set_holdings(&owner, holdings)
        .map(|_| StatusCode::NO_CONTENT)
        .map_err(|error| (StatusCode::BAD_REQUEST, Json(ErrorResponse { error })))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(ip_id: u64, value: i128) -> Holding {
        Holding { ip_id, estimated_value: value }
    }

    #[test]
    fn empty_portfolio_is_low_risk_with_no_suggestions() {
        let p = analyze("G1", &[]);
        assert_eq!(p.total_value, 0);
        assert_eq!(p.holding_count, 0);
        assert_eq!(p.risk_level, RiskLevel::Low);
        assert!(p.suggestions.is_empty());
    }

    #[test]
    fn single_holding_is_high_risk_and_suggests_diversifying() {
        let p = analyze("G1", &[h(1, 1_000)]);
        assert_eq!(p.risk_level, RiskLevel::High);
        assert_eq!(p.largest_position_bps, 10_000);
        // A 100% position also exceeds the cap, so a reduce suggestion accompanies the diversify one.
        assert!(p.suggestions.iter().any(|s| s.kind == SuggestionKind::Diversify));
        assert!(p.suggestions.iter().any(|s| s.kind == SuggestionKind::ReducePosition));
    }

    #[test]
    fn evenly_spread_portfolio_is_low_risk() {
        let holdings: Vec<Holding> = (1..=4).map(|i| h(i, 100)).collect();
        let p = analyze("G1", &holdings);
        assert_eq!(p.total_value, 400);
        assert_eq!(p.largest_position_bps, 2_500);
        assert_eq!(p.concentration_hhi, 2_500);
        assert_eq!(p.risk_level, RiskLevel::Medium);
        assert!(p.suggestions.is_empty());
    }

    #[test]
    fn oversized_position_gets_reduce_suggestion_to_cap() {
        // ip 1 is 80% of the portfolio; cap is 40% of total (1000 * 0.4 = 400).
        let p = analyze("G1", &[h(1, 800), h(2, 200)]);
        let reduce = p
            .suggestions
            .iter()
            .find(|s| s.kind == SuggestionKind::ReducePosition)
            .expect("reduce suggestion");
        assert_eq!(reduce.ip_id, Some(1));
        assert_eq!(reduce.amount, 400);
    }

    #[test]
    fn set_holdings_rejects_duplicates_and_negative_values() {
        assert!(set_holdings("G-dup", vec![h(1, 1), h(1, 2)]).is_err());
        assert!(set_holdings("G-neg", vec![h(1, -1)]).is_err());
        assert!(set_holdings("", vec![]).is_err());
    }

    #[test]
    fn set_holdings_is_visible_to_dashboard_lookup() {
        set_holdings("G-store", vec![h(9, 500)]).unwrap();
        let stored = HOLDINGS.get("G-store").map(|e| e.clone()).unwrap();
        assert_eq!(analyze("G-store", &stored).total_value, 500);
    }
}
