// SPDX-License-Identifier: GPL-3.0-or-later
//! Post-processing shared by all importers: validation, deduplication and reference cleanup.

use crate::bundle::{ImportBundle, WarningKind};
use core_model::{Credential, GroupId, Host, HostId, KeyId, SecretRef, Validate};
use std::collections::{HashMap, HashSet};

/// Validates every entity, drops invalid ones, deduplicates hosts by
/// `(address, port, username)` and removes dangling references.
pub(crate) fn finalize(mut b: ImportBundle) -> ImportBundle {
    fix_groups(&mut b);
    let dropped = drop_invalid_hosts(&mut b);
    let remap = dedup_hosts(&mut b);
    fix_jump_chains(&mut b, &remap, &dropped);
    fix_forwards(&mut b, &remap);
    dedup_known_hosts(&mut b);
    b.identities.retain(|i| i.validate().is_ok());
    drop_unreferenced(&mut b);
    b
}

fn fix_groups(b: &mut ImportBundle) {
    let mut replacement: HashMap<GroupId, Option<GroupId>> = HashMap::new();
    let mut warnings = Vec::new();
    b.groups.retain(|g| match g.validate() {
        Ok(()) => true,
        Err(e) => {
            replacement.insert(g.id, g.parent);
            warnings.push((g.label.clone(), e.to_string()));
            false
        }
    });
    for (label, e) in warnings {
        b.warn(
            WarningKind::Invalid,
            None,
            Some(&label),
            format!("group skipped: {e}"),
        );
    }
    if replacement.is_empty() {
        return;
    }
    let resolve = |mut g: Option<GroupId>| {
        // Bounded: each step removes one dropped group from consideration.
        for _ in 0..=replacement.len() {
            match g.and_then(|id| replacement.get(&id)) {
                Some(parent) => g = *parent,
                None => return g,
            }
        }
        None
    };
    for h in &mut b.hosts {
        h.group = resolve(h.group);
    }
    let parents: Vec<Option<GroupId>> = b.groups.iter().map(|g| resolve(g.parent)).collect();
    for (g, p) in b.groups.iter_mut().zip(parents) {
        g.parent = p;
    }
}

fn drop_invalid_hosts(b: &mut ImportBundle) -> HashSet<HostId> {
    let mut dropped = HashSet::new();
    let mut warnings = Vec::new();
    b.hosts.retain(|h| match h.validate() {
        Ok(()) => true,
        Err(e) => {
            dropped.insert(h.id);
            warnings.push((h.label.clone(), e.to_string()));
            false
        }
    });
    for (label, e) in warnings {
        b.warn(
            WarningKind::Invalid,
            None,
            Some(&label),
            format!("host skipped: {e}"),
        );
    }
    dropped
}

/// The identity of a host for deduplication.
fn dedup_key(h: &Host) -> (String, u16, Option<String>) {
    let address = h
        .address
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .to_ascii_lowercase();
    if h.config.telnet.enabled == Some(true) {
        return (
            address,
            h.config.telnet.port.unwrap_or(23),
            h.config.telnet.username.clone(),
        );
    }
    let user = match &h.config.ssh.credential {
        Some(Credential::Inline { username, .. }) => username.clone(),
        Some(Credential::Identity { id }) => Some(format!("identity:{id}")),
        None => None,
    };
    (address, h.config.ssh.port.unwrap_or(22), user)
}

fn dedup_hosts(b: &mut ImportBundle) -> HashMap<HostId, HostId> {
    let mut seen: HashMap<(String, u16, Option<String>), (HostId, String)> = HashMap::new();
    let mut remap = HashMap::new();
    let mut warnings = Vec::new();
    b.hosts.retain(|h| {
        let key = dedup_key(h);
        if let Some((kept, kept_label)) = seen.get(&key) {
            remap.insert(h.id, *kept);
            warnings.push((h.label.clone(), kept_label.clone()));
            false
        } else {
            seen.insert(key, (h.id, h.label.clone()));
            true
        }
    });
    for (label, kept) in warnings {
        b.warn(
            WarningKind::Duplicate,
            None,
            Some(&label),
            format!("same address, port and user as {kept:?}; merged into it"),
        );
    }
    remap
}

fn fix_jump_chains(
    b: &mut ImportBundle,
    remap: &HashMap<HostId, HostId>,
    dropped: &HashSet<HostId>,
) {
    let mut warnings = Vec::new();
    for h in &mut b.hosts {
        let Some(chain) = &mut h.config.ssh.jump_chain else {
            continue;
        };
        if chain.iter().any(|id| dropped.contains(id)) {
            chain.clear();
            warnings.push((
                h.label.clone(),
                "a jump host was invalid; jump chain removed",
            ));
            continue;
        }
        for id in chain.iter_mut() {
            if let Some(new) = remap.get(id) {
                *id = *new;
            }
        }
        let own = h.id;
        if chain.contains(&own) {
            chain.clear();
            warnings.push((
                h.label.clone(),
                "jump chain led back to the host itself; removed",
            ));
        }
    }
    for (label, msg) in warnings {
        b.warn(WarningKind::Invalid, None, Some(&label), msg);
    }
}

fn fix_forwards(b: &mut ImportBundle, remap: &HashMap<HostId, HostId>) {
    let hosts: HashSet<HostId> = b.hosts.iter().map(|h| h.id).collect();
    let mut seen = HashSet::new();
    let mut warnings = Vec::new();
    b.forwards.retain_mut(|f| {
        if let Some(new) = remap.get(&f.host) {
            f.host = *new;
        }
        if !hosts.contains(&f.host) {
            return false;
        }
        if let Err(e) = f.validate() {
            warnings.push((f.label.clone(), e.to_string()));
            return false;
        }
        seen.insert((
            f.host,
            f.kind as u8,
            f.bind_address.clone(),
            f.bind_port,
            f.dest_host.clone(),
            f.dest_port,
        ))
    });
    for (label, e) in warnings {
        b.warn(
            WarningKind::Invalid,
            None,
            Some(&label),
            format!("port forward skipped: {e}"),
        );
    }
}

fn dedup_known_hosts(b: &mut ImportBundle) {
    let mut seen = HashSet::new();
    b.known_hosts.retain(|k| {
        seen.insert((
            k.pattern.clone(),
            k.key_type.clone(),
            k.public_key.clone(),
            k.marker.map(|m| m as u8),
        ))
    });
}

fn drop_unreferenced(b: &mut ImportBundle) {
    let mut secrets: HashSet<SecretRef> = HashSet::new();
    let mut keys: HashSet<KeyId> = HashSet::new();
    for h in &b.hosts {
        if let Some(Credential::Inline {
            password, keys: k, ..
        }) = &h.config.ssh.credential
        {
            secrets.extend(password.iter().copied());
            keys.extend(k.iter().copied());
        }
        if let Some(p) = &h.config.ssh.proxy {
            secrets.extend(p.password.iter().copied());
        }
        secrets.extend(h.config.telnet.password.iter().copied());
    }
    for g in &b.groups {
        if let Some(Credential::Inline {
            password, keys: k, ..
        }) = &g.config.ssh.credential
        {
            secrets.extend(password.iter().copied());
            keys.extend(k.iter().copied());
        }
    }
    for i in &b.identities {
        secrets.extend(i.password.iter().copied());
        keys.extend(i.keys.iter().copied());
    }
    b.secrets.retain(|s| secrets.contains(&s.slot));
    b.keys.retain(|k| keys.contains(&k.id));
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use crate::bundle::{KeyFileFormat, SecretKind};
    use core_model::{ForwardKind, Group, PortForwardRule, RuleId};
    use zeroize::Zeroizing;

    fn fwd(host: HostId) -> PortForwardRule {
        PortForwardRule {
            id: RuleId::new(),
            label: "f".into(),
            kind: ForwardKind::Dynamic,
            host,
            bind_address: "localhost".into(),
            bind_port: 1080,
            dest_host: None,
            dest_port: None,
            auto_start: false,
            created_at: 0,
            updated_at: 0,
        }
    }

    #[test]
    fn dedup_remaps_references_and_drops_orphans() {
        let mut b = ImportBundle::default();
        let a = Host::new("a", "Example.com");
        let mut dup = Host::new("a-dup", "example.com");
        let pw = b.add_secret(SecretKind::Password, "x", Zeroizing::new(b"pw".to_vec()));
        let key = b.key_for_path("/k", KeyFileFormat::OpenSsh);
        dup.config.ssh.credential = None;
        let mut c = Host::new("c", "c.example");
        c.config.ssh.jump_chain = Some(vec![dup.id]);
        let mut bad = Host::new("bad", "not a host");
        bad.config.ssh.credential = Some(Credential::Inline {
            username: None,
            password: Some(pw),
            keys: vec![key],
        });
        b.forwards.push(fwd(dup.id));
        b.forwards.push(fwd(a.id));
        b.forwards.push(fwd(bad.id));
        let (a_id, c_id) = (a.id, c.id);
        b.hosts = vec![a, dup, c, bad];
        let b = finalize(b);
        assert_eq!(b.hosts.len(), 2);
        let c = b.hosts.iter().find(|h| h.id == c_id).unwrap();
        assert_eq!(c.config.ssh.jump_chain, Some(vec![a_id]));
        assert_eq!(b.forwards.len(), 1, "remapped duplicate forward is merged");
        assert!(b.secrets.is_empty() && b.keys.is_empty());
        assert!(b.warnings.iter().any(|w| w.kind == WarningKind::Duplicate));
        assert!(b.warnings.iter().any(|w| w.kind == WarningKind::Invalid));
    }

    #[test]
    fn invalid_jump_host_clears_chain() {
        let mut b = ImportBundle::default();
        let bad = Host::new("bad", "-nope");
        let mut c = Host::new("c", "c.example");
        c.config.ssh.jump_chain = Some(vec![bad.id]);
        b.hosts = vec![bad, c];
        let b = finalize(b);
        assert_eq!(b.hosts.len(), 1);
        assert_eq!(b.hosts[0].config.ssh.jump_chain, Some(vec![]));
    }

    #[test]
    fn invalid_groups_are_spliced_out() {
        let mut b = ImportBundle::default();
        let top = Group::new("top");
        let mut mid = Group::new("bad\u{7}");
        mid.parent = Some(top.id);
        let mut leaf = Group::new("leaf");
        leaf.parent = Some(mid.id);
        let mut h = Host::new("h", "h.example");
        h.group = Some(mid.id);
        let top_id = top.id;
        b.groups = vec![top, mid, leaf];
        b.hosts = vec![h];
        let b = finalize(b);
        assert_eq!(b.groups.len(), 2);
        assert_eq!(b.groups[1].parent, Some(top_id));
        assert_eq!(b.hosts[0].group, Some(top_id));
    }
}
