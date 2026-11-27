// SPDX-License-Identifier: GPL-3.0-or-later
//! Reconciling a provider inventory with the hosts of the integration's group.
//!
//! # Ownership of fields
//!
//! Hosts are matched by [`Host::cloud_id`]. Only a few fields are *provider-managed*:
//! `label`, `address`, `os` and the provider tags. Everything else (config, notes, favorite,
//! credentials, ...) belongs to the user and is never touched.
//!
//! The provider-managed fields use a three-way merge against the values the provider reported
//! last time ([`ManagedState`], persisted by the app in `CloudIntegration.settings` under
//! [`MANAGED_STATE_KEY`]):
//!
//! * if the user has not changed a field since the last sync (current value == last provider
//!   value), it follows the provider;
//! * if the user changed it, the user's value wins and is kept on every later sync.
//!
//! # Tags
//!
//! `Host` has a single tag list. It is split on every sync into *provider tags* (the tags the
//! provider reported last time, recorded in [`ManagedState`]) and *user tags* (everything
//! else). The new list is the new provider tags followed by the user tags, de-duplicated. So
//! tags the user adds survive, and tags removed at the provider disappear. If the managed
//! state is lost, all existing tags are treated as user tags (nothing is lost, at worst stale
//! provider tags remain).
//!
//! Hosts in the group without a `cloud_id` (added by hand) are never modified or removed.

use crate::model::{CloudInstance, Inventory};
use core_model::{GroupId, Host, Millis, OsKind};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};

/// Key under which the app stores [`ManagedState`] in `CloudIntegration.settings`.
pub const MANAGED_STATE_KEY: &str = "managedState";

/// Provider-managed values as last reported by the provider, per cloud id.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedState {
    #[serde(default)]
    pub hosts: BTreeMap<String, ManagedFields>,
}

/// Provider-managed fields of one host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedFields {
    pub label: String,
    pub address: String,
    #[serde(default)]
    pub os: OsKind,
    #[serde(default)]
    pub tags: Vec<String>,
}

impl ManagedState {
    /// Reads the state from integration settings (missing or malformed → empty).
    #[must_use]
    pub fn from_settings(settings: &serde_json::Map<String, serde_json::Value>) -> Self {
        settings
            .get(MANAGED_STATE_KEY)
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default()
    }

    /// Writes the state into integration settings.
    pub fn write_to(&self, settings: &mut serde_json::Map<String, serde_json::Value>) {
        if let Ok(v) = serde_json::to_value(self) {
            settings.insert(MANAGED_STATE_KEY.to_owned(), v);
        }
    }
}

/// Changes needed to bring the group in line with the provider.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Diff {
    /// New hosts (already assigned to the group).
    pub add: Vec<Host>,
    /// Existing hosts with updated provider-managed fields.
    pub update: Vec<Host>,
    /// Hosts whose instance no longer exists at the provider.
    pub remove: Vec<Host>,
    /// The managed state to persist after applying the diff.
    pub state: ManagedState,
    /// Non-fatal problems (duplicate cloud ids, inventory warnings).
    pub warnings: Vec<String>,
}

impl Diff {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.add.is_empty() && self.update.is_empty() && self.remove.is_empty()
    }
}

fn now_ms() -> Millis {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as Millis)
}

fn merge_field<T: PartialEq + Clone>(current: &T, previous: Option<&T>, provider: &T) -> T {
    match previous {
        // The user overrode the field: keep it.
        Some(prev) if current != prev => current.clone(),
        _ => provider.clone(),
    }
}

fn dedupe(tags: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut seen = HashSet::new();
    tags.into_iter()
        .filter(|t| seen.insert(t.to_lowercase()))
        .collect()
}

/// Computes the [`Diff`] between a provider `inventory` and the `existing` hosts of the
/// integration's `group`, given the [`ManagedState`] from the previous sync.
#[must_use]
pub fn sync_group(
    inventory: &Inventory,
    existing: &[Host],
    group: GroupId,
    previous: &ManagedState,
) -> Diff {
    let now = now_ms();
    let mut diff = Diff {
        warnings: inventory.warnings.clone(),
        ..Diff::default()
    };

    let mut by_cloud_id: HashMap<&str, &Host> = HashMap::new();
    for h in existing {
        if let Some(cid) = h.cloud_id.as_deref()
            && by_cloud_id.insert(cid, h).is_some()
        {
            diff.warnings.push(format!(
                "several hosts share cloud id {cid}; only one is updated"
            ));
            // Keep the first one deterministically.
            if let Some(first) = existing.iter().find(|x| x.cloud_id.as_deref() == Some(cid)) {
                by_cloud_id.insert(cid, first);
            }
        }
    }

    let mut seen: HashSet<&str> = HashSet::new();
    for inst in &inventory.instances {
        if !seen.insert(inst.cloud_id.as_str()) {
            diff.warnings
                .push(format!("provider reported {} twice", inst.cloud_id));
            continue;
        }
        diff.state
            .hosts
            .insert(inst.cloud_id.clone(), managed_of(inst));
        match by_cloud_id.get(inst.cloud_id.as_str()) {
            None => diff.add.push(new_host(inst, group, now)),
            Some(host) => {
                let prev = previous.hosts.get(&inst.cloud_id);
                let mut h = (*host).clone();
                h.label = merge_field(&host.label, prev.map(|p| &p.label), &inst.label);
                h.address = merge_field(&host.address, prev.map(|p| &p.address), &inst.address);
                h.os = merge_field(&host.os, prev.map(|p| &p.os), &inst.os);
                let old_provider: HashSet<String> = prev
                    .map(|p| p.tags.iter().map(|t| t.to_lowercase()).collect())
                    .unwrap_or_default();
                let user_tags = host
                    .tags
                    .iter()
                    .filter(|t| !old_provider.contains(&t.to_lowercase()))
                    .cloned();
                h.tags = dedupe(inst.tags.iter().cloned().chain(user_tags));
                if h != **host {
                    h.updated_at = now;
                    diff.update.push(h);
                }
            }
        }
    }

    for h in existing {
        if let Some(cid) = h.cloud_id.as_deref()
            && !seen.contains(cid)
        {
            diff.remove.push(h.clone());
        }
    }
    diff
}

fn managed_of(inst: &CloudInstance) -> ManagedFields {
    ManagedFields {
        label: inst.label.clone(),
        address: inst.address.clone(),
        os: inst.os,
        tags: inst.tags.clone(),
    }
}

fn new_host(inst: &CloudInstance, group: GroupId, now: Millis) -> Host {
    let mut h = Host::new(inst.label.clone(), inst.address.clone());
    h.group = Some(group);
    h.tags = dedupe(inst.tags.iter().cloned());
    h.os = inst.os;
    h.cloud_id = Some(inst.cloud_id.clone());
    h.created_at = now;
    h.updated_at = now;
    if let Some(user) = &inst.username_hint {
        h.config.ssh.credential = Some(core_model::Credential::Inline {
            username: Some(user.clone()),
            password: None,
            keys: Vec::new(),
        });
    }
    h
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn inst(id: &str, label: &str, addr: &str, tags: &[&str]) -> CloudInstance {
        CloudInstance {
            cloud_id: id.into(),
            label: label.into(),
            address: addr.into(),
            public_ip: Some(addr.into()),
            private_ip: None,
            public_dns: None,
            tags: tags.iter().map(|t| (*t).to_owned()).collect(),
            os: OsKind::Ubuntu,
            region: None,
            username_hint: None,
        }
    }

    fn inv(instances: Vec<CloudInstance>) -> Inventory {
        Inventory {
            instances,
            warnings: vec![],
        }
    }

    #[test]
    fn add_update_remove_and_preserve_user_edits() {
        let group = GroupId::new();
        let first = inv(vec![
            inst("a", "web", "1.1.1.1", &["env:prod"]),
            inst("b", "db", "2.2.2.2", &["env:prod", "role:db"]),
        ]);
        let d1 = sync_group(&first, &[], group, &ManagedState::default());
        assert_eq!(d1.add.len(), 2);
        assert!(d1.add.iter().all(|h| h.group == Some(group)));
        let mut hosts = d1.add.clone();

        // The user renames "web", adds a tag and a note, and adds a manual host.
        let web = hosts.iter_mut().find(|h| h.label == "web").unwrap();
        web.label = "frontend".into();
        web.tags.push("mine".into());
        web.notes = Some("hello".into());
        web.favorite = true;
        let manual = Host::new("manual", "3.3.3.3");
        hosts.push(manual.clone());

        // Provider: "a" changes IP and tags, "b" is gone, "c" is new.
        let second = inv(vec![
            inst("a", "web-renamed", "9.9.9.9", &["env:staging"]),
            inst("c", "cache", "4.4.4.4", &[]),
        ]);
        let d2 = sync_group(&second, &hosts, group, &d1.state);
        assert_eq!(d2.add.len(), 1);
        assert_eq!(d2.add[0].cloud_id.as_deref(), Some("c"));
        assert_eq!(d2.remove.len(), 1);
        assert_eq!(d2.remove[0].cloud_id.as_deref(), Some("b"));
        assert_eq!(d2.update.len(), 1);
        let a = &d2.update[0];
        assert_eq!(a.label, "frontend", "user rename is kept");
        assert_eq!(a.address, "9.9.9.9", "address follows the provider");
        assert_eq!(a.tags, vec!["env:staging".to_owned(), "mine".to_owned()]);
        assert_eq!(a.notes.as_deref(), Some("hello"));
        assert!(a.favorite);
        assert!(!d2.remove.iter().any(|h| h.id == manual.id));
    }

    #[test]
    fn unchanged_hosts_produce_no_update_and_state_roundtrips() {
        let group = GroupId::new();
        let i = inv(vec![inst("a", "web", "1.1.1.1", &["k:v"])]);
        let d1 = sync_group(&i, &[], group, &ManagedState::default());
        let d2 = sync_group(&i, &d1.add, group, &d1.state);
        assert!(d2.is_empty());

        let mut settings = serde_json::Map::new();
        d2.state.write_to(&mut settings);
        assert_eq!(ManagedState::from_settings(&settings), d2.state);
        assert_eq!(
            ManagedState::from_settings(&serde_json::Map::new()),
            ManagedState::default()
        );
    }

    #[test]
    fn lost_state_keeps_existing_tags() {
        let group = GroupId::new();
        let mut h = Host::new("web", "1.1.1.1");
        h.cloud_id = Some("a".into());
        h.tags = vec!["old:tag".into(), "mine".into()];
        let i = inv(vec![inst("a", "web", "1.1.1.1", &["new:tag"])]);
        let d = sync_group(&i, &[h], group, &ManagedState::default());
        assert_eq!(
            d.update[0].tags,
            vec!["new:tag".to_owned(), "old:tag".into(), "mine".into()]
        );
    }

    #[test]
    fn duplicates_are_reported() {
        let group = GroupId::new();
        let mut h1 = Host::new("web", "1.1.1.1");
        h1.cloud_id = Some("a".into());
        let mut h2 = h1.clone();
        h2.id = core_model::HostId::new();
        let i = inv(vec![
            inst("a", "web", "1.1.1.1", &[]),
            inst("a", "web", "1.1.1.1", &[]),
        ]);
        let d = sync_group(&i, &[h1, h2], group, &ManagedState::default());
        assert_eq!(d.warnings.len(), 2);
        assert!(d.remove.is_empty());
    }
}
