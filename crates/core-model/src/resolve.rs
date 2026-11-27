// SPDX-License-Identifier: GPL-3.0-or-later
//! Configuration inheritance resolution.

use crate::config::{Defaults, HostConfig, Inherit};
use crate::entities::{Group, Host};
use crate::error::ModelError;
use crate::id::{GroupId, HostId};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// The effective configuration of a host after inheritance, plus provenance information.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct ResolvedConfig {
    pub host: HostId,
    pub address: String,
    pub config: HostConfig,
    /// Ancestor groups from closest to farthest.
    pub ancestry: Vec<GroupId>,
}

/// Resolves effective configurations from a snapshot of hosts and groups.
#[derive(Debug, Default)]
pub struct Resolver<'a> {
    hosts: HashMap<HostId, &'a Host>,
    groups: HashMap<GroupId, &'a Group>,
}

impl<'a> Resolver<'a> {
    #[must_use]
    pub fn new(
        hosts: impl IntoIterator<Item = &'a Host>,
        groups: impl IntoIterator<Item = &'a Group>,
    ) -> Self {
        Self {
            hosts: hosts.into_iter().map(|h| (h.id, h)).collect(),
            groups: groups.into_iter().map(|g| (g.id, g)).collect(),
        }
    }

    /// Ancestor chain of a group (starting with the group itself).
    pub fn group_ancestry(&self, start: Option<GroupId>) -> Result<Vec<GroupId>, ModelError> {
        let mut chain = Vec::new();
        let mut seen = HashSet::new();
        let mut cur = start;
        while let Some(id) = cur {
            if !seen.insert(id) {
                return Err(ModelError::GroupCycle);
            }
            let group = self
                .groups
                .get(&id)
                .ok_or_else(|| ModelError::UnknownReference(format!("group {id}")))?;
            chain.push(id);
            cur = group.parent;
        }
        Ok(chain)
    }

    /// Effective configuration for a group (used for group defaults editing previews).
    pub fn resolve_group(&self, id: GroupId) -> Result<HostConfig, ModelError> {
        let ancestry = self.group_ancestry(Some(id))?;
        Ok(self.layer(HostConfig::default(), &ancestry))
    }

    /// Effective configuration for a host.
    pub fn resolve(&self, id: HostId) -> Result<ResolvedConfig, ModelError> {
        let host = self
            .hosts
            .get(&id)
            .ok_or_else(|| ModelError::UnknownReference(format!("host {id}")))?;
        let ancestry = self.group_ancestry(host.group)?;
        let config = self.layer(host.config.clone(), &ancestry);
        Ok(ResolvedConfig {
            host: id,
            address: host.address.clone(),
            config,
            ancestry,
        })
    }

    fn layer(&self, mut cfg: HostConfig, ancestry: &[GroupId]) -> HostConfig {
        for gid in ancestry {
            if let Some(g) = self.groups.get(gid) {
                cfg.inherit_from(&g.config);
            }
        }
        cfg.inherit_from(&Defaults::config());
        cfg
    }

    /// Returns the ordered list of hops (jump hosts) to reach `target`, validating that the
    /// chain is acyclic and does not contain the target itself.
    pub fn jump_chain(&self, target: HostId) -> Result<Vec<HostId>, ModelError> {
        let resolved = self.resolve(target)?;
        let chain = resolved.config.ssh.jump_chain.unwrap_or_default();
        let mut seen = HashSet::from([target]);
        for hop in &chain {
            if !seen.insert(*hop) {
                return Err(ModelError::ChainCycle);
            }
            if !self.hosts.contains_key(hop) {
                return Err(ModelError::UnknownReference(format!("jump host {hop}")));
            }
        }
        Ok(chain)
    }

    /// Returns `true` if making `group` a child of `new_parent` would create a cycle.
    #[must_use]
    pub fn would_create_cycle(&self, group: GroupId, new_parent: Option<GroupId>) -> bool {
        match self.group_ancestry(new_parent) {
            Ok(chain) => chain.contains(&group),
            Err(_) => true,
        }
    }

    /// All hosts in a group *and its subgroups* (groups are not folders).
    #[must_use]
    pub fn hosts_in_group_recursive(&self, group: GroupId) -> Vec<HostId> {
        let mut out: Vec<HostId> = self
            .hosts
            .values()
            .filter(|h| {
                self.group_ancestry(h.group)
                    .is_ok_and(|chain| chain.contains(&group))
            })
            .map(|h| h.id)
            .collect();
        out.sort();
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Credential, SshConfig, TerminalPrefs};

    fn fixture() -> (Vec<Host>, Vec<Group>) {
        let mut prod = Group::new("Production");
        prod.config.terminal = TerminalPrefs {
            theme: Some("red-alert".into()),
            ..TerminalPrefs::default()
        };
        prod.config.ssh = SshConfig {
            credential: Some(Credential::Inline {
                username: Some("deploy".into()),
                password: None,
                keys: vec![],
            }),
            port: Some(2200),
            ..SshConfig::default()
        };
        let mut eu = Group::new("EU");
        eu.parent = Some(prod.id);
        eu.config.ssh.port = Some(2222);

        let mut web = Host::new("web-1", "10.0.0.1");
        web.group = Some(eu.id);
        let mut db = Host::new("db-1", "10.0.0.2");
        db.group = Some(prod.id);
        db.config.ssh.port = Some(5022);
        let bastion = Host::new("bastion", "bastion.example.com");
        (vec![web, db, bastion], vec![prod, eu])
    }

    #[test]
    fn per_parameter_inheritance() {
        let (hosts, groups) = fixture();
        let r = Resolver::new(&hosts, &groups);
        let web = r.resolve(hosts[0].id).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(web.config.ssh.port, Some(2222), "closest group wins");
        assert_eq!(web.config.terminal.theme.as_deref(), Some("red-alert"));
        assert_eq!(web.ancestry, vec![groups[1].id, groups[0].id]);
        assert_eq!(
            web.config.ssh.keepalive_secs,
            Some(Defaults::KEEPALIVE_SECS)
        );

        let db = r.resolve(hosts[1].id).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(db.config.ssh.port, Some(5022), "host override wins");
        assert!(matches!(
            db.config.ssh.credential,
            Some(Credential::Inline { username: Some(ref u), .. }) if u == "deploy"
        ));

        let bastion = r.resolve(hosts[2].id).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(bastion.config.ssh.port, Some(22));
        assert_eq!(
            bastion.config.terminal.theme.as_deref(),
            Some(Defaults::THEME)
        );
    }

    #[test]
    fn detects_group_cycles() {
        let mut a = Group::new("a");
        let mut b = Group::new("b");
        a.parent = Some(b.id);
        b.parent = Some(a.id);
        let mut h = Host::new("h", "h");
        h.group = Some(a.id);
        let hosts = [h];
        let groups = [a, b];
        let r = Resolver::new(&hosts, &groups);
        assert_eq!(r.resolve(hosts[0].id), Err(ModelError::GroupCycle));
    }

    #[test]
    fn cycle_prevention_on_move() {
        let (hosts, groups) = fixture();
        let r = Resolver::new(&hosts, &groups);
        let (prod, eu) = (groups[0].id, groups[1].id);
        assert!(r.would_create_cycle(prod, Some(eu)));
        assert!(!r.would_create_cycle(eu, None));
        assert!(r.would_create_cycle(prod, Some(prod)));
    }

    #[test]
    fn jump_chain_validation() {
        let (mut hosts, groups) = fixture();
        let bastion = hosts[2].id;
        let web = hosts[0].id;
        hosts[0].config.ssh.jump_chain = Some(vec![bastion]);
        {
            let r = Resolver::new(&hosts, &groups);
            assert_eq!(r.jump_chain(web), Ok(vec![bastion]));
        }
        hosts[0].config.ssh.jump_chain = Some(vec![bastion, web]);
        let r = Resolver::new(&hosts, &groups);
        assert_eq!(r.jump_chain(web), Err(ModelError::ChainCycle));
    }

    #[test]
    fn recursive_group_membership() {
        let (hosts, groups) = fixture();
        let r = Resolver::new(&hosts, &groups);
        let in_prod = r.hosts_in_group_recursive(groups[0].id);
        assert_eq!(in_prod.len(), 2);
        let in_eu = r.hosts_in_group_recursive(groups[1].id);
        assert_eq!(in_eu, vec![hosts[0].id]);
    }
}
