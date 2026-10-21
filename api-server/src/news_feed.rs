//! IP news feed and notifications (#1099).
//!
//! Marketplace events (commitments, transfers, revocations and price changes)
//! are recorded here so users can follow what happens to their IPs:
//!
//! * **Activity feed** — the events for IPs a user currently owns, newest first.
//! * **Notifications** — price changes on the user's IPs, filtered by the
//!   user's [`NotificationPreferences`] (on/off and a minimum change in bps).
//! * **Marketplace trends** — aggregate counts and the average price move over
//!   a time window.
//!
//! The store is in memory and shared behind a [`Mutex`]. It is independent of
//! the HTTP layer so it can be wired into handlers without changing its logic.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, PoisonError};

use serde::{Deserialize, Serialize};

/// A single marketplace event that can appear in a feed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FeedEvent {
    Committed {
        ip_id: u64,
    },
    Transferred {
        ip_id: u64,
        to_owner: String,
    },
    Revoked {
        ip_id: u64,
    },
    PriceChanged {
        ip_id: u64,
        old_price: u64,
        new_price: u64,
    },
}

impl FeedEvent {
    /// The IP this event is about.
    pub fn ip_id(&self) -> u64 {
        match self {
            FeedEvent::Committed { ip_id }
            | FeedEvent::Transferred { ip_id, .. }
            | FeedEvent::Revoked { ip_id }
            | FeedEvent::PriceChanged { ip_id, .. } => *ip_id,
        }
    }
}

/// A recorded event with its sequence number and timestamp.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeedItem {
    pub seq: u64,
    pub timestamp: u64,
    pub event: FeedEvent,
}

/// Per-user notification settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotificationPreferences {
    /// Whether price-change notifications are delivered at all.
    pub price_changes: bool,
    /// Smallest price move, in basis points, that produces a notification.
    pub min_change_bps: u32,
}

impl Default for NotificationPreferences {
    fn default() -> Self {
        Self {
            price_changes: true,
            min_change_bps: 0,
        }
    }
}

/// Aggregate marketplace activity over a time window.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketTrends {
    pub window_start: u64,
    pub window_end: u64,
    pub commits: usize,
    pub transfers: usize,
    pub revocations: usize,
    pub price_changes: usize,
    /// Mean signed price move across all price changes, in basis points.
    /// `0` when there were no price changes.
    pub avg_price_change_bps: i64,
}

#[derive(Default)]
struct FeedState {
    next_seq: u64,
    items: Vec<FeedItem>,
    ip_owners: HashMap<u64, String>,
    preferences: HashMap<String, NotificationPreferences>,
}

/// Thread-safe in-memory news feed.
#[derive(Default)]
pub struct NewsFeed {
    state: Mutex<FeedState>,
}

/// Basis points in one whole unit (100%).
const BPS_DENOMINATOR: u64 = 10_000;

/// Magnitude of a price move in basis points. A move from zero to any
/// non-zero price counts as a full 100% change.
fn change_bps(old: u64, new: u64) -> u64 {
    if old == new {
        0
    } else if old == 0 {
        BPS_DENOMINATOR
    } else {
        let diff = old.abs_diff(new) as u128;
        (diff * BPS_DENOMINATOR as u128 / old as u128) as u64
    }
}

impl NewsFeed {
    pub fn new() -> Self {
        Self::default()
    }

    fn lock(&self) -> MutexGuard<'_, FeedState> {
        // A panic while holding the lock leaves the data consistent enough to
        // keep serving reads, so recover from poisoning instead of propagating it.
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Record a marketplace event and return its sequence number.
    ///
    /// Transfers update the owner index so later activity is attributed to
    /// the new owner. Commitments register the owner for the IP.
    pub fn record(&self, timestamp: u64, event: FeedEvent) -> u64 {
        let mut state = self.lock();
        if let FeedEvent::Transferred { ip_id, to_owner } = &event {
            state.ip_owners.insert(*ip_id, to_owner.clone());
        }
        let seq = state.next_seq;
        state.next_seq += 1;
        state.items.push(FeedItem {
            seq,
            timestamp,
            event,
        });
        seq
    }

    /// Register `owner` as the owner of `ip_id`, used to route activity.
    pub fn set_owner(&self, ip_id: u64, owner: &str) {
        self.lock().ip_owners.insert(ip_id, owner.to_string());
    }

    /// Record a price change. Returns `None` when the price did not change,
    /// so no-op updates do not clutter the feed.
    pub fn record_price_change(
        &self,
        timestamp: u64,
        ip_id: u64,
        old_price: u64,
        new_price: u64,
    ) -> Option<u64> {
        if old_price == new_price {
            return None;
        }
        Some(self.record(
            timestamp,
            FeedEvent::PriceChanged {
                ip_id,
                old_price,
                new_price,
            },
        ))
    }

    /// Store notification preferences for `owner`.
    pub fn set_preferences(&self, owner: &str, prefs: NotificationPreferences) {
        self.lock().preferences.insert(owner.to_string(), prefs);
    }

    /// Preferences for `owner`, or the defaults if none were set.
    pub fn preferences(&self, owner: &str) -> NotificationPreferences {
        self.lock()
            .preferences
            .get(owner)
            .cloned()
            .unwrap_or_default()
    }

    /// Events for IPs currently owned by `owner`, newest first, up to `limit`.
    pub fn activity_feed(&self, owner: &str, limit: usize) -> Vec<FeedItem> {
        let state = self.lock();
        state
            .items
            .iter()
            .rev()
            .filter(|item| {
                state.ip_owners.get(&item.event.ip_id()).map(String::as_str) == Some(owner)
            })
            .take(limit)
            .cloned()
            .collect()
    }

    /// Price-change notifications for `owner`'s IPs that pass their
    /// preferences, newest first, up to `limit`.
    pub fn notifications(&self, owner: &str, limit: usize) -> Vec<FeedItem> {
        let state = self.lock();
        let prefs = state.preferences.get(owner).cloned().unwrap_or_default();
        if !prefs.price_changes {
            return Vec::new();
        }
        state
            .items
            .iter()
            .rev()
            .filter(|item| {
                state.ip_owners.get(&item.event.ip_id()).map(String::as_str) == Some(owner)
            })
            .filter(|item| match item.event {
                FeedEvent::PriceChanged {
                    old_price,
                    new_price,
                    ..
                } => change_bps(old_price, new_price) >= u64::from(prefs.min_change_bps),
                _ => false,
            })
            .take(limit)
            .cloned()
            .collect()
    }

    /// Marketplace trends for items with `start <= timestamp <= end`.
    pub fn trends(&self, start: u64, end: u64) -> MarketTrends {
        let state = self.lock();
        let mut trends = MarketTrends {
            window_start: start,
            window_end: end,
            ..Default::default()
        };
        let mut signed_total: i128 = 0;

        for item in state
            .items
            .iter()
            .filter(|i| i.timestamp >= start && i.timestamp <= end)
        {
            match item.event {
                FeedEvent::Committed { .. } => trends.commits += 1,
                FeedEvent::Transferred { .. } => trends.transfers += 1,
                FeedEvent::Revoked { .. } => trends.revocations += 1,
                FeedEvent::PriceChanged {
                    old_price,
                    new_price,
                    ..
                } => {
                    trends.price_changes += 1;
                    let bps = change_bps(old_price, new_price) as i128;
                    signed_total += if new_price >= old_price { bps } else { -bps };
                }
            }
        }

        if trends.price_changes > 0 {
            trends.avg_price_change_bps = (signed_total / trends.price_changes as i128) as i64;
        }
        trends
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn committed(feed: &NewsFeed, ts: u64, owner: &str, ip_id: u64) {
        feed.set_owner(ip_id, owner);
        feed.record(ts, FeedEvent::Committed { ip_id });
    }

    #[test]
    fn activity_feed_is_newest_first_and_scoped_to_owner() {
        let feed = NewsFeed::new();
        committed(&feed, 1, "alice", 1);
        committed(&feed, 2, "bob", 2);
        committed(&feed, 3, "alice", 3);

        let items = feed.activity_feed("alice", 10);
        let ips: Vec<u64> = items.iter().map(|i| i.event.ip_id()).collect();
        assert_eq!(ips, vec![3, 1]);
    }

    #[test]
    fn limit_caps_the_feed() {
        let feed = NewsFeed::new();
        for ip in 1..=5 {
            committed(&feed, ip, "alice", ip);
        }
        assert_eq!(feed.activity_feed("alice", 2).len(), 2);
    }

    #[test]
    fn transfer_moves_future_activity_to_new_owner() {
        let feed = NewsFeed::new();
        committed(&feed, 1, "alice", 7);
        feed.record(
            2,
            FeedEvent::Transferred {
                ip_id: 7,
                to_owner: "bob".into(),
            },
        );
        feed.record(3, FeedEvent::Revoked { ip_id: 7 });

        // Alice no longer sees the IP. Bob sees its full history, including
        // the commitment that happened before he owned it.
        assert!(feed.activity_feed("alice", 10).is_empty());
        assert_eq!(feed.activity_feed("bob", 10).len(), 3);
    }

    #[test]
    fn unchanged_price_records_nothing() {
        let feed = NewsFeed::new();
        assert_eq!(feed.record_price_change(1, 1, 500, 500), None);
        assert!(feed.activity_feed("alice", 10).is_empty());
    }

    #[test]
    fn notifications_respect_minimum_change() {
        let feed = NewsFeed::new();
        feed.set_owner(1, "alice");
        feed.set_preferences(
            "alice",
            NotificationPreferences {
                price_changes: true,
                min_change_bps: 1_000, // 10%
            },
        );
        feed.record_price_change(1, 1, 1_000, 1_050); // 5%: below threshold
        feed.record_price_change(2, 1, 1_000, 1_200); // 20%: passes

        let notes = feed.notifications("alice", 10);
        assert_eq!(notes.len(), 1);
        assert_eq!(
            notes[0].event,
            FeedEvent::PriceChanged {
                ip_id: 1,
                old_price: 1_000,
                new_price: 1_200
            }
        );
    }

    #[test]
    fn disabled_price_notifications_return_nothing() {
        let feed = NewsFeed::new();
        feed.set_owner(1, "alice");
        feed.set_preferences(
            "alice",
            NotificationPreferences {
                price_changes: false,
                min_change_bps: 0,
            },
        );
        feed.record_price_change(1, 1, 100, 200);
        assert!(feed.notifications("alice", 10).is_empty());
        // Price changes still appear in the activity feed.
        assert_eq!(feed.activity_feed("alice", 10).len(), 1);
    }

    #[test]
    fn default_preferences_notify_on_any_change() {
        let feed = NewsFeed::new();
        assert_eq!(
            feed.preferences("nobody"),
            NotificationPreferences::default()
        );
        feed.set_owner(1, "alice");
        feed.record_price_change(1, 1, 100, 101);
        assert_eq!(feed.notifications("alice", 10).len(), 1);
    }

    #[test]
    fn change_bps_handles_zero_and_direction() {
        assert_eq!(change_bps(100, 100), 0);
        assert_eq!(change_bps(0, 5), BPS_DENOMINATOR);
        assert_eq!(change_bps(200, 100), 5_000);
        assert_eq!(change_bps(100, 150), 5_000);
    }

    #[test]
    fn trends_count_events_in_window_and_average_moves() {
        let feed = NewsFeed::new();
        feed.record(10, FeedEvent::Committed { ip_id: 1 });
        feed.record(
            20,
            FeedEvent::Transferred {
                ip_id: 1,
                to_owner: "b".into(),
            },
        );
        feed.record(30, FeedEvent::Revoked { ip_id: 1 });
        feed.record_price_change(40, 1, 100, 150); // +5000 bps
        feed.record_price_change(50, 1, 100, 50); // -5000 bps
        feed.record(999, FeedEvent::Committed { ip_id: 2 }); // outside window

        let t = feed.trends(0, 100);
        assert_eq!(t.commits, 1);
        assert_eq!(t.transfers, 1);
        assert_eq!(t.revocations, 1);
        assert_eq!(t.price_changes, 2);
        assert_eq!(t.avg_price_change_bps, 0);
    }

    #[test]
    fn trends_with_no_price_changes_average_to_zero() {
        let feed = NewsFeed::new();
        feed.record(1, FeedEvent::Committed { ip_id: 1 });
        let t = feed.trends(0, 10);
        assert_eq!(t.commits, 1);
        assert_eq!(t.avg_price_change_bps, 0);
    }

    #[test]
    fn feed_events_round_trip_through_json() {
        let event = FeedEvent::Transferred {
            ip_id: 9,
            to_owner: "carol".into(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"kind\":\"transferred\""));
        let back: FeedEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(back, event);
    }
}
