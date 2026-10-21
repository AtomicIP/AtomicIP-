//! IP collections and portfolios (#1098).
//!
//! An owner groups commitments into a named collection, shares it read-only
//! with other Stellar accounts, and runs collection-level operations: bulk
//! add/remove and a summary of the members.
//!
//! Every endpoint sits behind the request-signature middleware, and the caller
//! is identified by the verified `X-Public-Key` header, never by a body field.
//! A caller can read a collection only if they own it or it has been shared
//! with them. Only the owner can change, share, or delete it.

use axum::{
    extract::Path,
    http::{HeaderMap, StatusCode},
    Json,
};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::schemas::{ErrorResponse, IpRecord};

pub const MAX_NAME_LENGTH: usize = 100;
pub const MAX_IPS_PER_COLLECTION: usize = 500;

type Store = BTreeMap<String, StoredCollection>;

#[derive(Debug, Clone)]
struct StoredCollection {
    id: String,
    owner: String,
    name: String,
    ip_ids: BTreeSet<u64>,
    shared_with: BTreeSet<String>,
    created_at: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Collection {
    pub id: String,
    pub owner: String,
    pub name: String,
    pub ip_ids: Vec<u64>,
    pub shared_with: Vec<String>,
    pub created_at: u64,
}

impl From<&StoredCollection> for Collection {
    fn from(stored: &StoredCollection) -> Self {
        Collection {
            id: stored.id.clone(),
            owner: stored.owner.clone(),
            name: stored.name.clone(),
            ip_ids: stored.ip_ids.iter().copied().collect(),
            shared_with: stored.shared_with.iter().cloned().collect(),
            created_at: stored.created_at,
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TagCount {
    pub tag: String,
    pub count: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CollectionSummary {
    pub collection_id: String,
    pub ip_count: u64,
    pub revoked_count: u64,
    /// Members with no indexed commitment record.
    pub unresolved_ip_ids: Vec<u64>,
    pub tag_counts: Vec<TagCount>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum CollectionError {
    Invalid(String),
    NotFound,
    Forbidden,
}

impl From<CollectionError> for (StatusCode, Json<ErrorResponse>) {
    fn from(err: CollectionError) -> Self {
        let (status, message) = match err {
            CollectionError::Invalid(message) => (StatusCode::BAD_REQUEST, message),
            CollectionError::NotFound => (StatusCode::NOT_FOUND, "collection not found".to_string()),
            CollectionError::Forbidden => (
                StatusCode::FORBIDDEN,
                "only the collection owner can change this collection".to_string(),
            ),
        };
        (status, Json(ErrorResponse { error: message }))
    }
}

static COLLECTIONS: Lazy<Mutex<Store>> = Lazy::new(|| Mutex::new(BTreeMap::new()));

fn with_store<T>(f: impl FnOnce(&mut Store) -> Result<T, CollectionError>) -> Result<T, CollectionError> {
    let mut guard = COLLECTIONS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    f(&mut guard)
}

fn validate_name(name: &str) -> Result<String, CollectionError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(CollectionError::Invalid("collection name must not be empty".to_string()));
    }
    if trimmed.chars().count() > MAX_NAME_LENGTH {
        return Err(CollectionError::Invalid(format!(
            "collection name must be at most {MAX_NAME_LENGTH} characters"
        )));
    }
    Ok(trimmed.to_string())
}

fn validate_public_key(key: &str, role: &str) -> Result<(), CollectionError> {
    if stellar_strkey::ed25519::PublicKey::from_string(key).is_ok() {
        Ok(())
    } else {
        Err(CollectionError::Invalid(format!("{role} must be a valid Stellar public key")))
    }
}

fn check_capacity(count: usize) -> Result<(), CollectionError> {
    if count > MAX_IPS_PER_COLLECTION {
        return Err(CollectionError::Invalid(format!(
            "a collection may hold at most {MAX_IPS_PER_COLLECTION} IPs"
        )));
    }
    Ok(())
}

/// The collection, if `who` owns it. A reader who is not the owner gets
/// `Forbidden`. Anyone with no access gets `NotFound`, so existence is not leaked.
fn owned_mut<'a>(store: &'a mut Store, id: &str, who: &str) -> Result<&'a mut StoredCollection, CollectionError> {
    let collection = store.get_mut(id).ok_or(CollectionError::NotFound)?;
    if collection.owner == who {
        return Ok(collection);
    }
    if collection.shared_with.contains(who) {
        Err(CollectionError::Forbidden)
    } else {
        Err(CollectionError::NotFound)
    }
}

fn readable<'a>(store: &'a Store, id: &str, who: &str) -> Result<&'a StoredCollection, CollectionError> {
    store
        .get(id)
        .filter(|collection| collection.owner == who || collection.shared_with.contains(who))
        .ok_or(CollectionError::NotFound)
}

fn create(
    store: &mut Store,
    owner: &str,
    name: &str,
    ip_ids: &[u64],
    created_at: u64,
) -> Result<Collection, CollectionError> {
    let name = validate_name(name)?;
    let ip_ids: BTreeSet<u64> = ip_ids.iter().copied().collect();
    check_capacity(ip_ids.len())?;
    let id = uuid::Uuid::new_v4().to_string();
    let stored = StoredCollection {
        id: id.clone(),
        owner: owner.to_string(),
        name,
        ip_ids,
        shared_with: BTreeSet::new(),
        created_at,
    };
    let view = Collection::from(&stored);
    store.insert(id, stored);
    Ok(view)
}

fn get(store: &Store, id: &str, who: &str) -> Result<Collection, CollectionError> {
    readable(store, id, who).map(Collection::from)
}

/// Collections `who` owns or has been granted access to, oldest first.
fn list_for(store: &Store, who: &str) -> Vec<Collection> {
    let mut visible: Vec<&StoredCollection> = store
        .values()
        .filter(|collection| collection.owner == who || collection.shared_with.contains(who))
        .collect();
    visible.sort_by(|a, b| a.created_at.cmp(&b.created_at).then_with(|| a.id.cmp(&b.id)));
    visible.into_iter().map(Collection::from).collect()
}

fn add_ips(store: &mut Store, id: &str, who: &str, ip_ids: &[u64]) -> Result<Collection, CollectionError> {
    let collection = owned_mut(store, id, who)?;
    let merged: BTreeSet<u64> = collection.ip_ids.iter().copied().chain(ip_ids.iter().copied()).collect();
    check_capacity(merged.len())?;
    collection.ip_ids = merged;
    Ok(Collection::from(&*collection))
}

/// Removing an IP that is not in the collection succeeds unchanged, so the call is idempotent.
fn remove_ip(store: &mut Store, id: &str, who: &str, ip_id: u64) -> Result<Collection, CollectionError> {
    let collection = owned_mut(store, id, who)?;
    collection.ip_ids.remove(&ip_id);
    Ok(Collection::from(&*collection))
}

fn share(store: &mut Store, id: &str, who: &str, target: &str) -> Result<Collection, CollectionError> {
    validate_public_key(target, "share target")?;
    if target == who {
        return Err(CollectionError::Invalid("a collection cannot be shared with its owner".to_string()));
    }
    let collection = owned_mut(store, id, who)?;
    collection.shared_with.insert(target.to_string());
    Ok(Collection::from(&*collection))
}

fn unshare(store: &mut Store, id: &str, who: &str, target: &str) -> Result<Collection, CollectionError> {
    let collection = owned_mut(store, id, who)?;
    collection.shared_with.remove(target);
    Ok(Collection::from(&*collection))
}

fn delete(store: &mut Store, id: &str, who: &str) -> Result<(), CollectionError> {
    owned_mut(store, id, who)?;
    store.remove(id);
    Ok(())
}

/// Counts for a collection's members. Revocation and tags come from the indexed commitments.
fn summary(
    store: &Store,
    id: &str,
    who: &str,
    index: &[IpRecord],
    tags_of: &dyn Fn(u64) -> Vec<String>,
) -> Result<CollectionSummary, CollectionError> {
    let collection = readable(store, id, who)?;
    let by_id: HashMap<u64, &IpRecord> = index.iter().map(|record| (record.ip_id, record)).collect();

    let mut unresolved_ip_ids = Vec::new();
    let mut revoked_count = 0;
    let mut tags: BTreeMap<String, u64> = BTreeMap::new();
    for ip_id in &collection.ip_ids {
        match by_id.get(ip_id) {
            None => unresolved_ip_ids.push(*ip_id),
            Some(record) => {
                if record.revoked {
                    revoked_count += 1;
                }
                for tag in tags_of(*ip_id) {
                    *tags.entry(tag).or_insert(0) += 1;
                }
            }
        }
    }
    let mut tag_counts: Vec<TagCount> = tags
        .into_iter()
        .map(|(tag, count)| TagCount { tag, count })
        .collect();
    tag_counts.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.tag.cmp(&b.tag)));

    Ok(CollectionSummary {
        collection_id: collection.id.clone(),
        ip_count: collection.ip_ids.len() as u64,
        revoked_count,
        unresolved_ip_ids,
        tag_counts,
    })
}

// ── HTTP layer ────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CreateCollectionRequest {
    pub name: String,
    #[serde(default)]
    pub ip_ids: Vec<u64>,
}

#[derive(Debug, Deserialize)]
pub struct IpIdsRequest {
    pub ip_ids: Vec<u64>,
}

#[derive(Debug, Deserialize)]
pub struct ShareRequest {
    pub user: String,
}

type HttpError = (StatusCode, Json<ErrorResponse>);

/// The caller's Stellar key, from the `X-Public-Key` header. Routes using this are signed,
/// so the signature middleware has already proven control of the key.
fn requester(headers: &HeaderMap) -> Result<String, HttpError> {
    let key = headers
        .get("X-Public-Key")
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse {
                    error: "X-Public-Key header is required".to_string(),
                }),
            )
        })?;
    validate_public_key(key, "X-Public-Key").map_err(HttpError::from)?;
    Ok(key.to_string())
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0)
}

pub async fn create_collection(
    headers: HeaderMap,
    Json(body): Json<CreateCollectionRequest>,
) -> Result<Json<Collection>, HttpError> {
    let who = requester(&headers)?;
    with_store(|store| create(store, &who, &body.name, &body.ip_ids, now_secs()))
        .map(Json)
        .map_err(HttpError::from)
}

pub async fn list_collections(headers: HeaderMap) -> Result<Json<Vec<Collection>>, HttpError> {
    let who = requester(&headers)?;
    let collections = with_store(|store| Ok(list_for(store, &who))).map_err(HttpError::from)?;
    Ok(Json(collections))
}

pub async fn get_collection(
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Collection>, HttpError> {
    let who = requester(&headers)?;
    with_store(|store| get(store, &id, &who))
        .map(Json)
        .map_err(HttpError::from)
}

pub async fn delete_collection(headers: HeaderMap, Path(id): Path<String>) -> Result<StatusCode, HttpError> {
    let who = requester(&headers)?;
    with_store(|store| delete(store, &id, &who)).map_err(HttpError::from)?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn add_collection_ips(
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<IpIdsRequest>,
) -> Result<Json<Collection>, HttpError> {
    let who = requester(&headers)?;
    with_store(|store| add_ips(store, &id, &who, &body.ip_ids))
        .map(Json)
        .map_err(HttpError::from)
}

pub async fn remove_collection_ip(
    headers: HeaderMap,
    Path((id, ip_id)): Path<(String, u64)>,
) -> Result<Json<Collection>, HttpError> {
    let who = requester(&headers)?;
    with_store(|store| remove_ip(store, &id, &who, ip_id))
        .map(Json)
        .map_err(HttpError::from)
}

pub async fn share_collection(
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<ShareRequest>,
) -> Result<Json<Collection>, HttpError> {
    let who = requester(&headers)?;
    with_store(|store| share(store, &id, &who, &body.user))
        .map(Json)
        .map_err(HttpError::from)
}

pub async fn unshare_collection(
    headers: HeaderMap,
    Path((id, user)): Path<(String, String)>,
) -> Result<Json<Collection>, HttpError> {
    let who = requester(&headers)?;
    with_store(|store| unshare(store, &id, &who, &user))
        .map(Json)
        .map_err(HttpError::from)
}

pub async fn collection_summary(
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<CollectionSummary>, HttpError> {
    let who = requester(&headers)?;
    let index = crate::handlers::indexed_records();
    let result = with_store(|store| summary(store, &id, &who, &index, &crate::commitments::get_tags))
        .map_err(HttpError::from)?;
    Ok(Json(result))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(seed: u8) -> String {
        stellar_strkey::ed25519::PublicKey([seed; 32]).to_string()
    }

    fn ip(ip_id: u64, revoked: bool) -> IpRecord {
        IpRecord {
            ip_id,
            owner: key(1),
            commitment_hash: String::new(),
            timestamp: 1,
            revoked,
        }
    }

    fn no_tags(_: u64) -> Vec<String> {
        Vec::new()
    }

    #[test]
    fn owner_can_read_their_collection() {
        let mut store = Store::new();
        let created = create(&mut store, &key(1), "  Patents  ", &[], 10).unwrap();
        assert_eq!(created.name, "Patents");
        assert_eq!(get(&store, &created.id, &key(1)).unwrap(), created);
    }

    #[test]
    fn stranger_gets_not_found() {
        let mut store = Store::new();
        let created = create(&mut store, &key(1), "Patents", &[], 10).unwrap();
        assert_eq!(get(&store, &created.id, &key(2)), Err(CollectionError::NotFound));
    }

    #[test]
    fn shared_user_can_read_but_not_change() {
        let mut store = Store::new();
        let created = create(&mut store, &key(1), "Patents", &[1], 10).unwrap();
        share(&mut store, &created.id, &key(1), &key(2)).unwrap();
        assert!(get(&store, &created.id, &key(2)).is_ok());
        assert_eq!(
            add_ips(&mut store, &created.id, &key(2), &[9]),
            Err(CollectionError::Forbidden)
        );
        assert_eq!(
            delete(&mut store, &created.id, &key(2)),
            Err(CollectionError::Forbidden)
        );
    }

    #[test]
    fn unsharing_revokes_read_access() {
        let mut store = Store::new();
        let created = create(&mut store, &key(1), "Patents", &[], 10).unwrap();
        share(&mut store, &created.id, &key(1), &key(2)).unwrap();
        unshare(&mut store, &created.id, &key(1), &key(2)).unwrap();
        assert_eq!(get(&store, &created.id, &key(2)), Err(CollectionError::NotFound));
    }

    #[test]
    fn only_owner_can_share() {
        let mut store = Store::new();
        let created = create(&mut store, &key(1), "Patents", &[], 10).unwrap();
        share(&mut store, &created.id, &key(1), &key(2)).unwrap();
        assert_eq!(
            share(&mut store, &created.id, &key(2), &key(3)),
            Err(CollectionError::Forbidden)
        );
    }

    #[test]
    fn create_rejects_blank_and_overlong_names() {
        let mut store = Store::new();
        assert!(matches!(create(&mut store, &key(1), "   ", &[], 0), Err(CollectionError::Invalid(_))));
        let long = "x".repeat(MAX_NAME_LENGTH + 1);
        assert!(matches!(create(&mut store, &key(1), &long, &[], 0), Err(CollectionError::Invalid(_))));
    }

    #[test]
    fn ip_ids_are_deduplicated_and_sorted() {
        let mut store = Store::new();
        let created = create(&mut store, &key(1), "Patents", &[3, 1, 3], 0).unwrap();
        assert_eq!(created.ip_ids, vec![1, 3]);
    }

    #[test]
    fn collections_are_capped_in_size() {
        let mut store = Store::new();
        let too_many: Vec<u64> = (0..=MAX_IPS_PER_COLLECTION as u64).collect();
        assert!(matches!(
            create(&mut store, &key(1), "Big", &too_many, 0),
            Err(CollectionError::Invalid(_))
        ));
    }

    #[test]
    fn bulk_add_and_remove_members() {
        let mut store = Store::new();
        let created = create(&mut store, &key(1), "Patents", &[1, 2], 0).unwrap();
        let added = add_ips(&mut store, &created.id, &key(1), &[5]).unwrap();
        assert_eq!(added.ip_ids, vec![1, 2, 5]);
        let removed = remove_ip(&mut store, &created.id, &key(1), 1).unwrap();
        assert_eq!(removed.ip_ids, vec![2, 5]);
        let idempotent = remove_ip(&mut store, &created.id, &key(1), 1).unwrap();
        assert_eq!(idempotent.ip_ids, vec![2, 5]);
    }

    #[test]
    fn delete_removes_the_collection() {
        let mut store = Store::new();
        let created = create(&mut store, &key(1), "Patents", &[], 0).unwrap();
        delete(&mut store, &created.id, &key(1)).unwrap();
        assert_eq!(get(&store, &created.id, &key(1)), Err(CollectionError::NotFound));
    }

    #[test]
    fn list_includes_owned_and_shared_only() {
        let mut store = Store::new();
        let owned = create(&mut store, &key(1), "Mine", &[], 1).unwrap();
        let shared = create(&mut store, &key(2), "Theirs", &[], 2).unwrap();
        create(&mut store, &key(3), "Private", &[], 3).unwrap();
        share(&mut store, &shared.id, &key(2), &key(1)).unwrap();
        let visible: Vec<String> = list_for(&store, &key(1)).into_iter().map(|c| c.id).collect();
        assert_eq!(visible, vec![owned.id, shared.id]);
    }

    #[test]
    fn share_rejects_invalid_and_self_targets() {
        let mut store = Store::new();
        let created = create(&mut store, &key(1), "Patents", &[], 0).unwrap();
        assert!(matches!(
            share(&mut store, &created.id, &key(1), "not-a-key"),
            Err(CollectionError::Invalid(_))
        ));
        assert!(matches!(
            share(&mut store, &created.id, &key(1), &key(1)),
            Err(CollectionError::Invalid(_))
        ));
    }

    #[test]
    fn summary_counts_members_revocations_and_tags() {
        let mut store = Store::new();
        let created = create(&mut store, &key(1), "Patents", &[1, 2, 9], 0).unwrap();
        let index = vec![ip(1, false), ip(2, true)];
        let tags = |id: u64| match id {
            1 => vec!["art".to_string()],
            2 => vec!["art".to_string(), "music".to_string()],
            _ => Vec::new(),
        };
        let summary = summary(&store, &created.id, &key(1), &index, &tags).unwrap();
        assert_eq!(summary.ip_count, 3);
        assert_eq!(summary.revoked_count, 1);
        assert_eq!(summary.unresolved_ip_ids, vec![9]);
        assert_eq!(
            summary.tag_counts,
            vec![
                TagCount { tag: "art".to_string(), count: 2 },
                TagCount { tag: "music".to_string(), count: 1 },
            ]
        );
    }

    #[test]
    fn summary_requires_read_access() {
        let mut store = Store::new();
        let created = create(&mut store, &key(1), "Patents", &[1], 0).unwrap();
        assert_eq!(
            summary(&store, &created.id, &key(2), &[], &no_tags),
            Err(CollectionError::NotFound)
        );
    }
}
