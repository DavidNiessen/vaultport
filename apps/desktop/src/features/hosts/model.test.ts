// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from 'vitest';

import { allTags, groupPath, hostCounts, hostsInView, isWithin, newGroup, newHost } from './model';

function fixture() {
  const prod = { ...newGroup(null), label: 'Prod' };
  const eu = { ...newGroup(prod.id), label: 'EU' };
  const lab = { ...newGroup(null), label: 'Lab' };
  const web = { ...newHost(eu.id), label: 'web-1', address: '10.0.0.1', tags: ['web', 'Nginx'] };
  const db = { ...newHost(prod.id), label: 'db-1', address: '10.0.0.2', tags: ['db'] };
  const pi = { ...newHost(lab.id), label: 'pi', address: '192.168.1.2', tags: [] };
  return { groups: [prod, eu, lab], hosts: [web, db, pi], prod, eu, lab };
}

describe('host model', () => {
  it('computes group paths and containment', () => {
    const { groups, prod, eu } = fixture();
    expect(groupPath(groups, eu.id).map((g) => g.label)).toEqual(['Prod', 'EU']);
    expect(isWithin(groups, eu.id, prod.id)).toBe(true);
    expect(isWithin(groups, prod.id, eu.id)).toBe(false);
  });

  it('tolerates cycles', () => {
    const a = newGroup(null);
    const b = newGroup(a.id);
    a.parent = b.id;
    expect(groupPath([a, b], a.id)).toHaveLength(2);
  });

  it('shows subgroup hosts in a group view (groups are not folders)', () => {
    const { groups, hosts, prod } = fixture();
    expect(hostsInView(hosts, groups, prod.id, '', []).map((h) => h.label)).toEqual([
      'db-1',
      'web-1',
    ]);
  });

  it('filters by query terms and AND-combined tags', () => {
    const { groups, hosts } = fixture();
    expect(hostsInView(hosts, groups, null, 'nginx', []).map((h) => h.label)).toEqual(['web-1']);
    expect(hostsInView(hosts, groups, null, '10.0 db', []).map((h) => h.label)).toEqual(['db-1']);
    expect(hostsInView(hosts, groups, null, '', ['web', 'nginx'])).toHaveLength(1);
    expect(hostsInView(hosts, groups, null, '', ['web', 'db'])).toHaveLength(0);
  });

  it('counts hosts recursively and lists tags', () => {
    const { groups, hosts, prod, eu } = fixture();
    const counts = hostCounts(hosts, groups);
    expect(counts.get(prod.id)).toBe(2);
    expect(counts.get(eu.id)).toBe(1);
    expect(allTags(hosts)).toEqual(['db', 'Nginx', 'web']);
  });
});
