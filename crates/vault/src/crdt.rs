// SPDX-License-Identifier: GPL-3.0-or-later
//! A per-field last-writer-wins document CRDT.
//!
//! Each record is a JSON object flattened into leaf *paths* (`/config/ssh/port`). Every leaf is
//! an LWW register keyed by [`Hlc`]. A record-level tombstone hides the record when it is newer
//! than every field write. Applying the same set of operations in any order yields the same
//! document, which is what makes serverless multi-device sync merge-free.

use crate::hlc::Hlc;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use uuid::Uuid;

/// One change to a record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum Change {
    /// Sets a leaf. `Value::Null` means "unset".
    Set { path: String, value: Value },
    /// Deletes the whole record.
    Delete,
}

/// An operation in the replicated log.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Op {
    pub id: Uuid,
    pub hlc: Hlc,
    pub record: String,
    pub kind: String,
    pub change: Change,
}

/// A leaf register.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Register {
    pub hlc: Hlc,
    pub value: Value,
}

/// Materialized state of one record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Doc {
    pub kind: String,
    pub fields: BTreeMap<String, Register>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deleted: Option<Hlc>,
}

impl Doc {
    #[must_use]
    pub fn new(kind: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            fields: BTreeMap::new(),
            deleted: None,
        }
    }

    /// Applies an operation. Returns `true` if the visible state may have changed.
    pub fn apply(&mut self, hlc: Hlc, change: &Change) -> bool {
        match change {
            Change::Set { path, value } => match self.fields.get(path) {
                Some(reg) if reg.hlc >= hlc => false,
                _ => {
                    self.fields.insert(
                        path.clone(),
                        Register {
                            hlc,
                            value: value.clone(),
                        },
                    );
                    true
                }
            },
            Change::Delete => {
                if self.deleted.is_some_and(|d| d >= hlc) {
                    false
                } else {
                    self.deleted = Some(hlc);
                    true
                }
            }
        }
    }

    /// Latest write of any kind.
    #[must_use]
    pub fn latest(&self) -> Hlc {
        let f = self
            .fields
            .values()
            .map(|r| r.hlc)
            .max()
            .unwrap_or(Hlc::ZERO);
        self.deleted.map_or(f, |d| d.max(f))
    }

    /// A record is deleted if its tombstone is newer than every field write ("delete wins
    /// against older edits, newer edits resurrect").
    #[must_use]
    pub fn is_deleted(&self) -> bool {
        match self.deleted {
            None => false,
            Some(d) => self.fields.values().all(|r| r.hlc <= d),
        }
    }

    /// Rebuilds the JSON value of the record (`None` when deleted).
    #[must_use]
    pub fn to_value(&self) -> Option<Value> {
        if self.is_deleted() {
            return None;
        }
        let mut root = Value::Object(Map::new());
        for (path, reg) in &self.fields {
            if reg.value.is_null() {
                continue;
            }
            insert_path(&mut root, path, reg.value.clone());
        }
        Some(root)
    }
}

/// Escapes a JSON-pointer path segment.
fn escape(seg: &str) -> String {
    seg.replace('~', "~0").replace('/', "~1")
}

fn unescape(seg: &str) -> String {
    seg.replace("~1", "/").replace("~0", "~")
}

/// Flattens an object into `(path, leaf)` pairs. Arrays, scalars and empty objects are leaves.
#[must_use]
pub fn flatten(value: &Value) -> BTreeMap<String, Value> {
    let mut out = BTreeMap::new();
    flatten_into(value, String::new(), &mut out);
    out
}

fn flatten_into(value: &Value, prefix: String, out: &mut BTreeMap<String, Value>) {
    match value {
        Value::Object(map) if !map.is_empty() => {
            for (k, v) in map {
                flatten_into(v, format!("{prefix}/{}", escape(k)), out);
            }
        }
        Value::Object(_) if !prefix.is_empty() => {
            out.insert(prefix, Value::Object(Map::new()));
        }
        Value::Object(_) => {}
        other => {
            out.insert(prefix, other.clone());
        }
    }
}

fn insert_path(root: &mut Value, path: &str, value: Value) {
    let segs: Vec<String> = path.split('/').skip(1).map(unescape).collect();
    let mut cur = root;
    for (i, seg) in segs.iter().enumerate() {
        if !cur.is_object() {
            *cur = Value::Object(Map::new());
        }
        let Value::Object(map) = cur else { return };
        if i == segs.len() - 1 {
            match (map.get_mut(seg), value) {
                // An empty-object leaf must not clobber children set by other paths.
                (Some(existing @ Value::Object(_)), Value::Object(m)) if m.is_empty() => {
                    let _ = existing;
                }
                (_, v) => {
                    map.insert(seg.clone(), v);
                }
            }
            return;
        }
        cur = map
            .entry(seg.clone())
            .or_insert_with(|| Value::Object(Map::new()));
    }
}

/// Computes the `Set` changes that turn `old` into `new` (both objects).
#[must_use]
pub fn diff(old: Option<&Value>, new: &Value) -> Vec<Change> {
    let old_flat = old.map(flatten).unwrap_or_default();
    let new_flat = flatten(new);
    let mut changes = Vec::new();
    for (path, value) in &new_flat {
        if old_flat.get(path) != Some(value) {
            changes.push(Change::Set {
                path: path.clone(),
                value: value.clone(),
            });
        }
    }
    for path in old_flat.keys() {
        if !new_flat.contains_key(path) {
            changes.push(Change::Set {
                path: path.clone(),
                value: Value::Null,
            });
        }
    }
    changes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hlc::DeviceId;
    use proptest::prelude::*;
    use serde_json::json;

    fn h(ms: u64, dev: u64) -> Hlc {
        Hlc {
            wall_ms: ms,
            counter: 0,
            device: DeviceId(dev),
        }
    }

    fn materialize(ops: &[(Hlc, Change)]) -> Option<Value> {
        let mut d = Doc::new("host");
        for (hlc, c) in ops {
            d.apply(*hlc, c);
        }
        d.to_value()
    }

    #[test]
    fn flatten_roundtrip() {
        let v = json!({"label": "web", "config": {"ssh": {"port": 22, "env": [1, 2]}, "mosh": {}}, "a/b": {"~x": true}});
        let flat = flatten(&v);
        assert!(flat.contains_key("/config/ssh/port"));
        assert!(flat.contains_key("/a~1b/~0x"));
        let mut d = Doc::new("x");
        for (p, val) in flat {
            d.apply(
                h(1, 1),
                &Change::Set {
                    path: p,
                    value: val,
                },
            );
        }
        assert_eq!(d.to_value(), Some(v));
    }

    #[test]
    fn concurrent_edits_to_different_fields_merge() {
        let base =
            json!({"label": "web", "config": {"ssh": {"port": 22}, "terminal": {"theme": "a"}}});
        let a =
            json!({"label": "web", "config": {"ssh": {"port": 2222}, "terminal": {"theme": "a"}}});
        let b =
            json!({"label": "web", "config": {"ssh": {"port": 22}, "terminal": {"theme": "red"}}});
        let mut ops: Vec<(Hlc, Change)> = diff(None, &base)
            .into_iter()
            .map(|c| (h(1, 1), c))
            .collect();
        ops.extend(diff(Some(&base), &a).into_iter().map(|c| (h(2, 1), c)));
        ops.extend(diff(Some(&base), &b).into_iter().map(|c| (h(3, 2), c)));
        let merged = materialize(&ops);
        assert_eq!(
            merged,
            Some(
                json!({"label": "web", "config": {"ssh": {"port": 2222}, "terminal": {"theme": "red"}}})
            )
        );
    }

    #[test]
    fn unset_fields_disappear() {
        let a = json!({"label": "x", "notes": "hi"});
        let b = json!({"label": "x"});
        let mut ops: Vec<(Hlc, Change)> =
            diff(None, &a).into_iter().map(|c| (h(1, 1), c)).collect();
        ops.extend(diff(Some(&a), &b).into_iter().map(|c| (h(2, 1), c)));
        assert_eq!(materialize(&ops), Some(b));
    }

    #[test]
    fn delete_semantics() {
        let a = json!({"label": "x"});
        let mut ops: Vec<(Hlc, Change)> =
            diff(None, &a).into_iter().map(|c| (h(1, 1), c)).collect();
        ops.push((h(2, 2), Change::Delete));
        assert_eq!(materialize(&ops), None);
        // A newer edit after the delete resurrects the record.
        ops.push((
            h(3, 1),
            Change::Set {
                path: "/label".into(),
                value: json!("y"),
            },
        ));
        assert_eq!(materialize(&ops), Some(json!({"label": "y"})));
    }

    proptest! {
        #[test]
        fn convergence_is_order_independent(
            writes in proptest::collection::vec((0u64..20, 0u64..3, 0usize..4, 0i64..100, proptest::bool::weighted(0.1)), 1..40),
            seed in any::<u64>()
        ) {
            let paths = ["/a", "/b", "/c/d", "/c/e"];
            let ops: Vec<(Hlc, Change)> = writes
                .iter()
                .enumerate()
                .map(|(i, (ms, dev, p, v, del))| {
                    let hlc = Hlc { wall_ms: *ms, counter: i as u32, device: DeviceId(*dev) };
                    let change = if *del { Change::Delete } else {
                        Change::Set { path: paths[*p].to_string(), value: json!(v) }
                    };
                    (hlc, change)
                })
                .collect();
            let mut shuffled = ops.clone();
            // Deterministic Fisher-Yates using the seed.
            let mut s = seed | 1;
            for i in (1..shuffled.len()).rev() {
                s ^= s << 13; s ^= s >> 7; s ^= s << 17;
                let j = (s % (i as u64 + 1)) as usize;
                shuffled.swap(i, j);
            }
            prop_assert_eq!(materialize(&ops), materialize(&shuffled));
        }
    }
}
