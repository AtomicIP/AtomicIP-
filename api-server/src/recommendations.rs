//! IP recommendations (#1097).
//!
//! `GET /recommendations?user_id=...` suggests indexed commitments the user has
//! not watched yet. Two signals are blended equally:
//!
//! - **Content-based**: the share of a candidate's tags that also appear on the
//!   commitments the user already watches.
//! - **Collaborative**: the commitments watched by other users whose watchlists
//!   overlap with the user's. Each such user is weighted by the cosine similarity
//!   of the two watchlists.
//!
//! A user with no watchlist gets no recommendations. There is no popularity
//! fallback yet, so cold-start users see an empty list.

use axum::{extract::Query, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};

use crate::schemas::{ErrorResponse, IpRecord};

pub const DEFAULT_LIMIT: usize = 10;
pub const MAX_LIMIT: usize = 50;

type Watchlists = HashMap<String, BTreeSet<u64>>;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RecommendationSource {
    Content,
    Collaborative,
    Hybrid,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Recommendation {
    pub ip_id: u64,
    /// Blended score in `[0, 1]`. Higher is a stronger recommendation.
    pub score: f64,
    pub source: RecommendationSource,
    pub reasons: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct RecommendationQuery {
    pub user_id: String,
    pub limit: Option<usize>,
}

/// Rank unwatched, unrevoked candidates for `user_id`, best first.
///
/// `tags_of` supplies each commitment's tags. Ties on score break on `ip_id`
/// so results are stable.
pub fn recommend(
    user_id: &str,
    watchlists: &Watchlists,
    candidates: &[IpRecord],
    tags_of: &dyn Fn(u64) -> Vec<String>,
    limit: usize,
) -> Vec<Recommendation> {
    let watched = match watchlists.get(user_id) {
        Some(set) if !set.is_empty() => set,
        _ => return Vec::new(),
    };
    let profile: BTreeSet<String> = watched.iter().flat_map(|id| tags_of(*id)).collect();

    // Users who share at least one watched commitment, weighted by cosine
    // similarity of their watchlists.
    let neighbours: Vec<(f64, &BTreeSet<u64>)> = watchlists
        .iter()
        .filter(|(other_id, other)| other_id.as_str() != user_id && !other.is_empty())
        .filter_map(|(_, other)| {
            let overlap = watched.intersection(other).count();
            (overlap > 0).then(|| {
                let similarity = overlap as f64 / ((watched.len() * other.len()) as f64).sqrt();
                (similarity, other)
            })
        })
        .collect();
    let total_weight: f64 = neighbours.iter().map(|(weight, _)| *weight).sum();

    let mut results: Vec<Recommendation> = candidates
        .iter()
        .filter(|record| !record.revoked && !watched.contains(&record.ip_id))
        .filter_map(|record| {
            let tags = tags_of(record.ip_id);
            let matched: BTreeSet<&str> = tags
                .iter()
                .filter(|tag| profile.contains(tag.as_str()))
                .map(String::as_str)
                .collect();
            let content = if tags.is_empty() {
                0.0
            } else {
                matched.len() as f64 / tags.len() as f64
            };

            let (endorsed_weight, endorsers) = neighbours
                .iter()
                .filter(|(_, other)| other.contains(&record.ip_id))
                .fold((0.0, 0usize), |(sum, count), (weight, _)| (sum + *weight, count + 1));
            let collaborative = if total_weight > 0.0 {
                endorsed_weight / total_weight
            } else {
                0.0
            };

            let source = match (content > 0.0, collaborative > 0.0) {
                (false, false) => return None,
                (true, true) => RecommendationSource::Hybrid,
                (true, false) => RecommendationSource::Content,
                (false, true) => RecommendationSource::Collaborative,
            };

            let mut reasons = Vec::new();
            if !matched.is_empty() {
                let shared: Vec<&str> = matched.iter().copied().collect();
                reasons.push(format!("shares tags: {}", shared.join(", ")));
            }
            if endorsers > 0 {
                reasons.push(format!("watched by {endorsers} user(s) with similar watchlists"));
            }

            Some(Recommendation {
                ip_id: record.ip_id,
                score: round4(0.5 * content + 0.5 * collaborative),
                source,
                reasons,
            })
        })
        .collect();

    results.sort_by(|a, b| b.score.total_cmp(&a.score).then_with(|| a.ip_id.cmp(&b.ip_id)));
    results.truncate(limit);
    results
}

fn round4(value: f64) -> f64 {
    (value * 10_000.0).round() / 10_000.0
}

/// Ranked recommendations for one watchlist user.
pub async fn get_recommendations(
    Query(query): Query<RecommendationQuery>,
) -> Result<Json<Vec<Recommendation>>, (StatusCode, Json<ErrorResponse>)> {
    let user_id = query.user_id.trim();
    if user_id.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "user_id must not be empty".to_string(),
            }),
        ));
    }
    let limit = query.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let watchlists = crate::handlers::watchlist_snapshot();
    let candidates = crate::handlers::indexed_records();
    Ok(Json(recommend(
        user_id,
        &watchlists,
        &candidates,
        &crate::commitments::get_tags,
        limit,
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(ip_id: u64) -> IpRecord {
        IpRecord {
            ip_id,
            owner: String::new(),
            commitment_hash: String::new(),
            timestamp: 1,
            revoked: false,
        }
    }

    fn lists(entries: &[(&str, &[u64])]) -> Watchlists {
        entries
            .iter()
            .map(|(user, ids)| (user.to_string(), ids.iter().copied().collect()))
            .collect()
    }

    fn tagger(entries: Vec<(u64, Vec<&str>)>) -> impl Fn(u64) -> Vec<String> {
        let map: HashMap<u64, Vec<String>> = entries
            .into_iter()
            .map(|(id, tags)| (id, tags.into_iter().map(String::from).collect()))
            .collect();
        move |id| map.get(&id).cloned().unwrap_or_default()
    }

    #[test]
    fn user_without_watchlist_gets_nothing() {
        let tags = tagger(vec![]);
        let watchlists = lists(&[]);
        assert!(recommend("zed", &watchlists, &[ip(1)], &tags, 10).is_empty());
    }

    #[test]
    fn user_with_empty_watchlist_gets_nothing() {
        let tags = tagger(vec![]);
        let watchlists = lists(&[("alice", &[])]);
        assert!(recommend("alice", &watchlists, &[ip(1)], &tags, 10).is_empty());
    }

    #[test]
    fn content_match_on_shared_tags() {
        let tags = tagger(vec![(1, vec!["art", "rare"]), (2, vec!["art"]), (3, vec!["music"])]);
        let watchlists = lists(&[("alice", &[1])]);
        let candidates = [ip(1), ip(2), ip(3)];
        let results = recommend("alice", &watchlists, &candidates, &tags, 10);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].ip_id, 2);
        assert_eq!(results[0].source, RecommendationSource::Content);
        assert_eq!(results[0].score, 0.5);
        assert_eq!(results[0].reasons, vec!["shares tags: art".to_string()]);
    }

    #[test]
    fn excludes_watched_and_revoked_candidates() {
        let tags = tagger(vec![(1, vec!["art"]), (2, vec!["art"]), (4, vec!["art"])]);
        let watchlists = lists(&[("alice", &[1])]);
        let mut revoked = ip(4);
        revoked.revoked = true;
        let candidates = [ip(1), revoked, ip(2)];
        let results = recommend("alice", &watchlists, &candidates, &tags, 10);
        let ids: Vec<u64> = results.iter().map(|r| r.ip_id).collect();
        assert_eq!(ids, vec![2]);
    }

    #[test]
    fn collaborative_signal_from_overlapping_watchlists() {
        let tags = tagger(vec![]);
        let watchlists = lists(&[("alice", &[1, 2]), ("bob", &[1, 2, 7])]);
        let results = recommend("alice", &watchlists, &[ip(7)], &tags, 10);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].ip_id, 7);
        assert_eq!(results[0].source, RecommendationSource::Collaborative);
        assert_eq!(results[0].score, 0.5);
        assert_eq!(
            results[0].reasons,
            vec!["watched by 1 user(s) with similar watchlists".to_string()]
        );
    }

    #[test]
    fn strong_candidates_rank_above_weaker_ones() {
        // Candidate 8 matches on tags and is watched by a similar user (hybrid).
        // Candidate 9 only matches on tags (content).
        let tags = tagger(vec![(1, vec!["art"]), (2, vec!["art"]), (8, vec!["art"]), (9, vec!["art"])]);
        let watchlists = lists(&[("alice", &[1, 2]), ("bob", &[1, 2, 8])]);
        let results = recommend("alice", &watchlists, &[ip(9), ip(8)], &tags, 10);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].ip_id, 8);
        assert_eq!(results[0].source, RecommendationSource::Hybrid);
        assert_eq!(results[0].score, 1.0);
        assert_eq!(results[1].ip_id, 9);
        assert_eq!(results[1].source, RecommendationSource::Content);
    }

    #[test]
    fn limit_truncates_ranked_results() {
        let tags = tagger(vec![(1, vec!["art"]), (2, vec!["art"]), (8, vec!["art"]), (9, vec!["art"])]);
        let watchlists = lists(&[("alice", &[1, 2]), ("bob", &[1, 2, 8])]);
        let results = recommend("alice", &watchlists, &[ip(9), ip(8)], &tags, 1);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].ip_id, 8);
    }

    #[test]
    fn scores_stay_within_unit_interval() {
        let tags = tagger(vec![(1, vec!["art"]), (3, vec!["art", "music"])]);
        let watchlists = lists(&[("alice", &[1]), ("bob", &[1, 3]), ("carol", &[1, 4, 5])]);
        let candidates = [ip(3), ip(4), ip(5)];
        for result in recommend("alice", &watchlists, &candidates, &tags, 10) {
            assert!((0.0..=1.0).contains(&result.score), "score {} out of range", result.score);
        }
    }
}
