//! #1090: IP marketplace search with filters, full-text metadata matching,
//! faceted results and saved searches.
//!
//! Matching and faceting are pure functions over `(IpRecord, metadata)` pairs so
//! they can be tested without the HTTP layer. Metadata and saved searches live in
//! process memory, the same way commitment tags do in `commitments.rs`.

use dashmap::DashMap;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use utoipa::{IntoParams, ToSchema};

use crate::schemas::IpRecord;

const MAX_PAGE_SIZE: usize = 100;
const DEFAULT_PAGE_SIZE: usize = 20;
const MAX_SAVED_SEARCHES_PER_OWNER: usize = 20;
const MAX_SEARCH_NAME_LENGTH: usize = 64;
const MAX_FIELD_LENGTH: usize = 256;

/// Marketplace-facing metadata attached to an IP record. Only the fields
/// search can filter on are stored here; the commitment itself stays on-chain.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct IpSearchMetadata {
    pub title: String,
    #[serde(default)]
    pub description: String,
    pub category: String,
    pub jurisdiction: String,
    /// Asking price in the smallest token unit, if the IP is listed for sale.
    #[serde(default)]
    pub price: Option<u64>,
}

/// Filters accepted by `GET /v1/ip/search` and stored in saved searches.
/// Every field is optional; unset fields do not constrain the result.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, ToSchema, IntoParams)]
pub struct IpSearchFilters {
    /// Whitespace-separated terms; every term must appear in the metadata.
    pub q: Option<String>,
    pub min_price: Option<u64>,
    pub max_price: Option<u64>,
    /// Inclusive lower bound on the registration timestamp (unix seconds).
    pub registered_after: Option<u64>,
    /// Inclusive upper bound on the registration timestamp (unix seconds).
    pub registered_before: Option<u64>,
    pub category: Option<String>,
    pub jurisdiction: Option<String>,
    /// Page size, capped at 100 (default 20).
    pub limit: Option<usize>,
    /// Number of matches to skip.
    pub offset: Option<usize>,
}

#[derive(Debug, Clone, Serialize, ToSchema, PartialEq)]
pub struct FacetCount {
    pub value: String,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, ToSchema, PartialEq, Default)]
pub struct SearchFacets {
    pub categories: Vec<FacetCount>,
    pub jurisdictions: Vec<FacetCount>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct IpSearchResponse {
    /// Total matches before pagination.
    pub total: usize,
    pub items: Vec<IpRecord>,
    /// Counts over all matches (not just the current page).
    pub facets: SearchFacets,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq)]
pub struct SavedSearch {
    pub id: u64,
    pub owner: String,
    pub name: String,
    pub filters: IpSearchFilters,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SaveSearchRequest {
    pub name: String,
    pub filters: IpSearchFilters,
}

/// Searchable listings keyed by ip_id: the record snapshot plus its metadata.
static LISTINGS: Lazy<DashMap<u64, (IpRecord, IpSearchMetadata)>> = Lazy::new(DashMap::new);
static SAVED_SEARCHES: Lazy<DashMap<String, Vec<SavedSearch>>> = Lazy::new(DashMap::new);
static NEXT_SAVED_SEARCH_ID: AtomicU64 = AtomicU64::new(1);

/// Indexes (or re-indexes) an IP for search. The record snapshot is stored with
/// the metadata so search does not depend on any other store. Category and
/// jurisdiction are normalized to lowercase so facets and filters are
/// case-insensitive.
pub fn index_listing(
    record: IpRecord,
    mut metadata: IpSearchMetadata,
) -> Result<IpSearchMetadata, String> {
    metadata.title = metadata.title.trim().to_string();
    metadata.description = metadata.description.trim().to_string();
    metadata.category = metadata.category.trim().to_ascii_lowercase();
    metadata.jurisdiction = metadata.jurisdiction.trim().to_ascii_lowercase();

    if metadata.title.is_empty() {
        return Err("title must not be empty".to_string());
    }
    if metadata.category.is_empty() || metadata.jurisdiction.is_empty() {
        return Err("category and jurisdiction must not be empty".to_string());
    }
    for field in [&metadata.title, &metadata.description, &metadata.category, &metadata.jurisdiction] {
        if field.len() > MAX_FIELD_LENGTH {
            return Err(format!("metadata fields must be at most {MAX_FIELD_LENGTH} bytes"));
        }
    }

    LISTINGS.insert(record.ip_id, (record, metadata.clone()));
    Ok(metadata)
}

/// Every indexed listing, in no particular order. Feed this to [`search`].
pub fn listings() -> Vec<(IpRecord, Option<IpSearchMetadata>)> {
    LISTINGS
        .iter()
        .map(|entry| {
            let (record, metadata) = entry.value().clone();
            (record, Some(metadata))
        })
        .collect()
}

pub fn get_metadata(ip_id: u64) -> Option<IpSearchMetadata> {
    LISTINGS.get(&ip_id).map(|entry| entry.value().1.clone())
}

/// Runs `filters` against `records`, returning a page of matches and facet counts
/// computed over every match. Results are ordered by `ip_id` so pagination is stable.
pub fn search(
    records: &[(IpRecord, Option<IpSearchMetadata>)],
    filters: &IpSearchFilters,
) -> IpSearchResponse {
    let terms: Vec<String> = filters
        .q
        .as_deref()
        .unwrap_or("")
        .split_whitespace()
        .map(|t| t.to_ascii_lowercase())
        .collect();
    let category = filters.category.as_ref().map(|c| c.trim().to_ascii_lowercase());
    let jurisdiction = filters.jurisdiction.as_ref().map(|j| j.trim().to_ascii_lowercase());

    let mut matches: Vec<&(IpRecord, Option<IpSearchMetadata>)> = records
        .iter()
        .filter(|(record, metadata)| {
            if let Some(after) = filters.registered_after {
                if record.timestamp < after {
                    return false;
                }
            }
            if let Some(before) = filters.registered_before {
                if record.timestamp > before {
                    return false;
                }
            }
            // Anything that depends on metadata excludes records without it.
            let needs_metadata = filters.min_price.is_some()
                || filters.max_price.is_some()
                || category.is_some()
                || jurisdiction.is_some()
                || !terms.is_empty();
            let Some(meta) = metadata else {
                return !needs_metadata;
            };
            if let Some(min) = filters.min_price {
                if meta.price.map_or(true, |p| p < min) {
                    return false;
                }
            }
            if let Some(max) = filters.max_price {
                if meta.price.map_or(true, |p| p > max) {
                    return false;
                }
            }
            if let Some(cat) = &category {
                if &meta.category != cat {
                    return false;
                }
            }
            if let Some(jur) = &jurisdiction {
                if &meta.jurisdiction != jur {
                    return false;
                }
            }
            let haystack = format!(
                "{} {} {} {}",
                meta.title.to_ascii_lowercase(),
                meta.description.to_ascii_lowercase(),
                meta.category,
                meta.jurisdiction
            );
            terms.iter().all(|term| haystack.contains(term.as_str()))
        })
        .collect();

    matches.sort_by_key(|(record, _)| record.ip_id);

    let facets = SearchFacets {
        categories: facet_counts(matches.iter().filter_map(|(_, m)| m.as_ref().map(|m| m.category.clone()))),
        jurisdictions: facet_counts(matches.iter().filter_map(|(_, m)| m.as_ref().map(|m| m.jurisdiction.clone()))),
    };

    let total = matches.len();
    let limit = filters.limit.unwrap_or(DEFAULT_PAGE_SIZE).clamp(1, MAX_PAGE_SIZE);
    let offset = filters.offset.unwrap_or(0);
    let items = matches
        .into_iter()
        .skip(offset)
        .take(limit)
        .map(|(record, _)| record.clone())
        .collect();

    IpSearchResponse { total, items, facets }
}

fn facet_counts(values: impl Iterator<Item = String>) -> Vec<FacetCount> {
    let mut counts: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    for value in values {
        *counts.entry(value).or_insert(0) += 1;
    }
    let mut facets: Vec<FacetCount> = counts
        .into_iter()
        .map(|(value, count)| FacetCount { value, count })
        .collect();
    // Most common first; ties keep alphabetical order from the BTreeMap.
    facets.sort_by(|a, b| b.count.cmp(&a.count));
    facets
}

/// Saves a named search for `owner`. Names must be unique per owner.
pub fn save_search(owner: &str, name: &str, filters: IpSearchFilters) -> Result<SavedSearch, String> {
    let owner = owner.trim();
    let name = name.trim();
    if owner.is_empty() {
        return Err("owner must not be empty".to_string());
    }
    if name.is_empty() || name.len() > MAX_SEARCH_NAME_LENGTH {
        return Err(format!("name must be 1-{MAX_SEARCH_NAME_LENGTH} characters"));
    }

    let mut entry = SAVED_SEARCHES.entry(owner.to_string()).or_default();
    if entry.iter().any(|s| s.name == name) {
        return Err(format!("a saved search named '{name}' already exists"));
    }
    if entry.len() >= MAX_SAVED_SEARCHES_PER_OWNER {
        return Err(format!("an owner may have at most {MAX_SAVED_SEARCHES_PER_OWNER} saved searches"));
    }

    let saved = SavedSearch {
        id: NEXT_SAVED_SEARCH_ID.fetch_add(1, Ordering::Relaxed),
        owner: owner.to_string(),
        name: name.to_string(),
        filters,
    };
    entry.push(saved.clone());
    Ok(saved)
}

pub fn list_saved_searches(owner: &str) -> Vec<SavedSearch> {
    SAVED_SEARCHES
        .get(owner.trim())
        .map(|entry| entry.value().clone())
        .unwrap_or_default()
}

/// Removes a saved search. Returns `false` if the owner has no search with that id.
pub fn delete_saved_search(owner: &str, id: u64) -> bool {
    let Some(mut entry) = SAVED_SEARCHES.get_mut(owner.trim()) else {
        return false;
    };
    let before = entry.len();
    entry.retain(|s| s.id != id);
    entry.len() != before
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(ip_id: u64, timestamp: u64) -> IpRecord {
        IpRecord {
            ip_id,
            owner: "GOWNER".to_string(),
            commitment_hash: format!("hash-{ip_id}"),
            timestamp,
            revoked: false,
        }
    }

    fn meta(title: &str, description: &str, category: &str, jurisdiction: &str, price: Option<u64>) -> IpSearchMetadata {
        IpSearchMetadata {
            title: title.to_string(),
            description: description.to_string(),
            category: category.to_string(),
            jurisdiction: jurisdiction.to_string(),
            price,
        }
    }

    fn dataset() -> Vec<(IpRecord, Option<IpSearchMetadata>)> {
        vec![
            (record(1, 100), Some(meta("Solar Cell", "thin film photovoltaic", "energy", "us", Some(500)))),
            (record(2, 200), Some(meta("Battery Pack", "lithium solar storage", "energy", "ng", Some(1500)))),
            (record(3, 300), Some(meta("Gear Train", "precision gearbox", "mechanical", "us", None))),
            (record(4, 400), None),
        ]
    }

    fn ids(resp: &IpSearchResponse) -> Vec<u64> {
        resp.items.iter().map(|r| r.ip_id).collect()
    }

    #[test]
    fn empty_filters_return_everything_including_unindexed_records() {
        let resp = search(&dataset(), &IpSearchFilters::default());
        assert_eq!(resp.total, 4);
        assert_eq!(ids(&resp), vec![1, 2, 3, 4]);
    }

    #[test]
    fn price_range_excludes_records_without_price() {
        let f = IpSearchFilters { min_price: Some(400), max_price: Some(1000), ..Default::default() };
        assert_eq!(ids(&search(&dataset(), &f)), vec![1]);
    }

    #[test]
    fn registration_date_bounds_are_inclusive() {
        let f = IpSearchFilters { registered_after: Some(200), registered_before: Some(300), ..Default::default() };
        assert_eq!(ids(&search(&dataset(), &f)), vec![2, 3]);
    }

    #[test]
    fn category_and_jurisdiction_are_case_insensitive() {
        let f = IpSearchFilters { category: Some("ENERGY".into()), jurisdiction: Some("Us".into()), ..Default::default() };
        assert_eq!(ids(&search(&dataset(), &f)), vec![1]);
    }

    #[test]
    fn full_text_requires_every_term_in_metadata() {
        let f = IpSearchFilters { q: Some("solar storage".into()), ..Default::default() };
        assert_eq!(ids(&search(&dataset(), &f)), vec![2]);
        let f = IpSearchFilters { q: Some("solar".into()), ..Default::default() };
        assert_eq!(ids(&search(&dataset(), &f)), vec![1, 2]);
    }

    #[test]
    fn facets_count_all_matches_not_just_the_page() {
        let f = IpSearchFilters { limit: Some(1), ..Default::default() };
        let resp = search(&dataset(), &f);
        assert_eq!(resp.items.len(), 1);
        assert_eq!(resp.total, 4);
        assert_eq!(
            resp.facets.categories,
            vec![
                FacetCount { value: "energy".into(), count: 2 },
                FacetCount { value: "mechanical".into(), count: 1 },
            ]
        );
        assert_eq!(resp.facets.jurisdictions.iter().map(|f| f.count).sum::<usize>(), 3);
    }

    #[test]
    fn pagination_is_stable_and_capped() {
        let f = IpSearchFilters { limit: Some(2), offset: Some(2), ..Default::default() };
        assert_eq!(ids(&search(&dataset(), &f)), vec![3, 4]);

        let f = IpSearchFilters { limit: Some(10_000), ..Default::default() };
        assert_eq!(search(&dataset(), &f).items.len(), 4);
    }

    #[test]
    fn metadata_validation_normalizes_and_rejects_blanks() {
        let stored = index_listing(record(9001, 1), meta("  Widget ", "", " Tools ", "US", Some(10))).unwrap();
        assert_eq!(stored.title, "Widget");
        assert_eq!(stored.category, "tools");
        assert_eq!(stored.jurisdiction, "us");
        assert_eq!(get_metadata(9001), Some(stored));
        assert!(index_listing(record(9002, 1), meta("  ", "", "tools", "us", None)).is_err());
        assert!(index_listing(record(9003, 1), meta("x", "", "", "us", None)).is_err());
    }

    #[test]
    fn saved_searches_are_per_owner_unique_and_deletable() {
        let owner = "GSAVER-TEST-1090";
        let f = IpSearchFilters { q: Some("solar".into()), ..Default::default() };
        let saved = save_search(owner, "solar deals", f.clone()).unwrap();
        assert!(save_search(owner, "solar deals", f.clone()).is_err());
        assert!(save_search("GOTHER-TEST-1090", "solar deals", f.clone()).is_ok());

        assert_eq!(list_saved_searches(owner), vec![saved.clone()]);
        assert!(delete_saved_search(owner, saved.id));
        assert!(!delete_saved_search(owner, saved.id));
        assert!(list_saved_searches(owner).is_empty());
    }

    #[test]
    fn saved_search_name_length_is_enforced() {
        let long = "n".repeat(MAX_SEARCH_NAME_LENGTH + 1);
        assert!(save_search("GLEN-TEST-1090", &long, IpSearchFilters::default()).is_err());
        assert!(save_search("", "ok", IpSearchFilters::default()).is_err());
    }
}
