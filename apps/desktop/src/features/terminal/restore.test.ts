// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from 'vitest';

import type { Tab, TerminalTab } from '@/stores/app';

import { restoreTabs, snapshot } from './restore';

const term = (id: string, extra: Partial<TerminalTab> = {}): TerminalTab => ({
  id,
  kind: 'terminal',
  title: id,
  hostId: `host-${id}`,
  protocol: 'ssh',
  sessionId: 'secret-session',
  status: 'connected',
  unread: true,
  ...extra,
});

describe('restoring tabs', () => {
  it('saves references only and restores tabs and splits as new connections', () => {
    const tabs: Tab[] = [
      term('a', { customTitle: true }),
      { id: 'log', kind: 'log', title: 'log', logId: 'l' },
      {
        id: 'w',
        kind: 'workspace',
        title: 'W',
        panes: ['p1', 'p2'],
        mode: 'focus',
        broadcast: true,
        focused: 'p2',
        templateId: null,
      },
      term('p1', { workspace: 'w' }),
      term('p2', { workspace: 'w', protocol: 'local', hostId: null, cwd: '/srv' }),
    ];
    const snap = snapshot(tabs, 'w');
    expect(snap.active).toBe(1);
    expect(JSON.stringify(snap)).not.toContain('secret-session');
    expect(snap.tabs.map((t) => t.kind)).toEqual(['terminal', 'workspace']);

    const { tabs: restored, active } = restoreTabs(snap);
    expect(restored).toHaveLength(4);
    const [a, ws, p1, p2] = restored;
    expect(a).toMatchObject({ kind: 'terminal', title: 'a', hostId: 'host-a', customTitle: true });
    expect(a?.kind === 'terminal' && a.status).toBe('connecting');
    expect(a?.kind === 'terminal' && a.sessionId).toBeNull();
    expect(ws?.kind === 'workspace' && ws.panes).toEqual([p1?.id, p2?.id]);
    // Broadcast is never restored on.
    expect(ws?.kind === 'workspace' && ws.broadcast).toBe(false);
    expect(p2).toMatchObject({ protocol: 'local', cwd: '/srv', workspace: ws?.id });
    expect(active).toBe(ws?.id);
  });
});
