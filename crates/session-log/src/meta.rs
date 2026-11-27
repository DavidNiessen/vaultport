// SPDX-License-Identifier: GPL-3.0-or-later
//! Log metadata (stored by the app in the vault) and retention.

use core_model::{HostId, Millis};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::str::FromStr;
use uuid::Uuid;

/// Identifier of a session log.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LogId(pub Uuid);

impl LogId {
    /// A new, time-ordered (UUIDv7) id.
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }

    #[must_use]
    pub const fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    #[must_use]
    pub const fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl Default for LogId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for LogId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for LogId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Uuid::parse_str(s).map(Self)
    }
}

/// Descriptive data about one recording. The app stores this (encrypted) in the vault; the
/// log file itself only carries the id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogMetadata {
    pub id: LogId,
    /// The saved host, if the session was opened from one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_id: Option<HostId>,
    /// Display label of the host at recording time.
    pub host_label: String,
    /// Milliseconds since the Unix epoch.
    pub started_at: Millis,
    /// `None` while recording (or if the app crashed).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<Millis>,
    /// Size of the encrypted log file in bytes.
    #[serde(default)]
    pub bytes: u64,
    /// Bookmarked logs are never deleted by retention.
    #[serde(default)]
    pub bookmarked: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    /// Name of the device that recorded the session.
    #[serde(default)]
    pub device_name: String,
}

/// Retention rules. Bookmarked logs are always kept and do not count toward
/// `keep_per_host`; they do count toward `max_total_bytes`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetentionPolicy {
    /// Keep at most this many non-bookmarked logs per host (logs without a host form one
    /// group).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keep_per_host: Option<usize>,
    /// Delete the oldest non-bookmarked logs until the total size fits.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_total_bytes: Option<u64>,
}

/// Returns the ids of logs to delete under `policy`, oldest first. Deterministic: ties on
/// `started_at` are broken by id. Callers should leave logs that are still being recorded out
/// of `logs`.
#[must_use]
pub fn select_for_deletion(logs: &[LogMetadata], policy: &RetentionPolicy) -> Vec<LogId> {
    // Newest first.
    let mut sorted: Vec<&LogMetadata> = logs.iter().collect();
    sorted.sort_by(|a, b| b.started_at.cmp(&a.started_at).then(b.id.cmp(&a.id)));
    sorted.dedup_by_key(|m| m.id);

    let mut delete: HashSet<LogId> = HashSet::new();
    if let Some(keep) = policy.keep_per_host {
        let mut seen: HashMap<Option<HostId>, usize> = HashMap::new();
        for m in sorted.iter().filter(|m| !m.bookmarked) {
            let n = seen.entry(m.host_id).or_insert(0);
            *n += 1;
            if *n > keep {
                delete.insert(m.id);
            }
        }
    }
    if let Some(max) = policy.max_total_bytes {
        let mut total: u64 = sorted
            .iter()
            .filter(|m| !delete.contains(&m.id))
            .map(|m| m.bytes)
            .fold(0, u64::saturating_add);
        for m in sorted.iter().rev() {
            if total <= max {
                break;
            }
            if m.bookmarked || delete.contains(&m.id) {
                continue;
            }
            delete.insert(m.id);
            total = total.saturating_sub(m.bytes);
        }
    }
    sorted
        .iter()
        .rev()
        .filter(|m| delete.contains(&m.id))
        .map(|m| m.id)
        .collect()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn log(n: u128, host: Option<HostId>, started: Millis, bytes: u64, bm: bool) -> LogMetadata {
        LogMetadata {
            id: LogId::from_uuid(Uuid::from_u128(n)),
            host_id: host,
            host_label: "h".into(),
            started_at: started,
            ended_at: Some(started + 10),
            bytes,
            bookmarked: bm,
            comment: None,
            device_name: "laptop".into(),
        }
    }

    fn id(n: u128) -> LogId {
        LogId::from_uuid(Uuid::from_u128(n))
    }

    #[test]
    fn keep_n_per_host_skips_bookmarks() {
        let a = Some(HostId::from_uuid(Uuid::from_u128(100)));
        let b = Some(HostId::from_uuid(Uuid::from_u128(200)));
        let logs = vec![
            log(1, a, 1_000, 10, false),
            log(2, a, 2_000, 10, true),
            log(3, a, 3_000, 10, false),
            log(4, a, 4_000, 10, false),
            log(5, b, 1_500, 10, false),
            log(6, None, 1_000, 10, false),
            log(7, None, 2_000, 10, false),
        ];
        let policy = RetentionPolicy {
            keep_per_host: Some(2),
            max_total_bytes: None,
        };
        assert_eq!(select_for_deletion(&logs, &policy), [id(1)]);
        let policy = RetentionPolicy {
            keep_per_host: Some(1),
            max_total_bytes: None,
        };
        assert_eq!(select_for_deletion(&logs, &policy), [id(1), id(6), id(3)]);
    }

    #[test]
    fn max_total_bytes_deletes_oldest_first() {
        let logs = vec![
            log(1, None, 1_000, 100, true),
            log(2, None, 2_000, 100, false),
            log(3, None, 3_000, 100, false),
            log(4, None, 4_000, 100, false),
        ];
        let policy = RetentionPolicy {
            keep_per_host: None,
            max_total_bytes: Some(250),
        };
        assert_eq!(select_for_deletion(&logs, &policy), [id(2), id(3)]);
        let policy = RetentionPolicy {
            keep_per_host: None,
            max_total_bytes: Some(0),
        };
        // The bookmarked log survives even if the budget cannot be met.
        assert_eq!(select_for_deletion(&logs, &policy), [id(2), id(3), id(4)]);
        assert!(select_for_deletion(&logs, &RetentionPolicy::default()).is_empty());
    }

    #[test]
    fn metadata_serde() {
        let m = log(9, None, 5, 7, false);
        let json = serde_json::to_string(&m).unwrap();
        assert!(json.contains("\"hostLabel\":\"h\""));
        assert!(json.contains("\"deviceName\":\"laptop\""));
        let back: LogMetadata = serde_json::from_str(&json).unwrap();
        assert_eq!(back, m);
        assert_eq!(m.id.to_string().parse::<LogId>().unwrap(), m.id);
    }
}
