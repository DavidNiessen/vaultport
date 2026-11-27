// SPDX-License-Identifier: GPL-3.0-or-later
/** Pure helpers for hosts and groups (tested in model.test.ts). */
import { emptyConfig, newId, type Group, type Host } from '@/lib/ipc';

export function newHost(group: string | null): Host {
  return {
    id: newId(),
    label: '',
    address: '',
    group,
    tags: [],
    config: emptyConfig(),
    os: 'unknown',
    favorite: false,
    notes: null,
    cloudId: null,
    createdAt: 0,
    updatedAt: 0,
    lastConnectedAt: null,
  };
}

export function newGroup(parent: string | null): Group {
  return {
    id: newId(),
    label: '',
    parent,
    config: emptyConfig(),
    integration: null,
    createdAt: 0,
    updatedAt: 0,
  };
}

/** Ancestors of a group from root to the group itself. */
export function groupPath(groups: Group[], id: string | null): Group[] {
  const byId = new Map(groups.map((g) => [g.id, g]));
  const path: Group[] = [];
  const seen = new Set<string>();
  let cur = id ? byId.get(id) : undefined;
  while (cur && !seen.has(cur.id)) {
    seen.add(cur.id);
    path.unshift(cur);
    cur = cur.parent ? byId.get(cur.parent) : undefined;
  }
  return path;
}

/** Whether `groupId` is `ancestor` or one of its descendants. */
export function isWithin(
  groups: Group[],
  groupId: string | null | undefined,
  ancestor: string,
): boolean {
  return groupPath(groups, groupId ?? null).some((g) => g.id === ancestor);
}

/** Hosts visible in a group view: the group's hosts *and* those of all subgroups. */
export function hostsInView(
  hosts: Host[],
  groups: Group[],
  current: string | null,
  query: string,
  tags: string[],
): Host[] {
  const q = query.trim().toLowerCase();
  const terms = q ? q.split(/\s+/) : [];
  return hosts
    .filter((h) => (current ? isWithin(groups, h.group, current) : true))
    .filter((h) => tags.every((t) => h.tags.some((x) => x.toLowerCase() === t.toLowerCase())))
    .filter((h) =>
      terms.every(
        (term) =>
          h.label.toLowerCase().includes(term) ||
          h.address.toLowerCase().includes(term) ||
          h.tags.some((t) => t.toLowerCase().includes(term)),
      ),
    )
    .sort(
      (a, b) =>
        Number(b.favorite) - Number(a.favorite) ||
        a.label.localeCompare(b.label, undefined, { numeric: true, sensitivity: 'base' }),
    );
}

export function childGroups(groups: Group[], parent: string | null): Group[] {
  return groups
    .filter((g) => (g.parent ?? null) === parent)
    .sort((a, b) => a.label.localeCompare(b.label, undefined, { numeric: true }));
}

/** Counts hosts per group including subgroups. */
export function hostCounts(hosts: Host[], groups: Group[]): Map<string, number> {
  const counts = new Map<string, number>();
  for (const h of hosts) {
    for (const g of groupPath(groups, h.group ?? null)) {
      counts.set(g.id, (counts.get(g.id) ?? 0) + 1);
    }
  }
  return counts;
}

export function allTags(hosts: Host[]): string[] {
  const set = new Map<string, string>();
  for (const h of hosts) for (const t of h.tags) set.set(t.toLowerCase(), t);
  return [...set.values()].sort((a, b) => a.localeCompare(b));
}

/** Username shown on a host card (inline credential or identity). */
export function displayUser(h: Host): string | null {
  const c = h.config.ssh.credential;
  if (c?.type === 'inline') return c.username ?? null;
  return null;
}

/** Second line of a host card: protocol, user and tags. */
export function hostSubtitle(h: Host): string {
  const parts: string[] = [];
  parts.push(
    h.config.mosh.enabled
      ? 'mosh'
      : h.config.telnet.enabled && h.config.ssh.enabled === false
        ? 'telnet'
        : 'ssh',
  );
  const u = displayUser(h);
  if (u) parts.push(u);
  parts.push(...h.tags);
  return parts.join(', ');
}
