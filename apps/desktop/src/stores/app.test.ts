// SPDX-License-Identifier: GPL-3.0-or-later
import { beforeEach, describe, expect, it } from 'vitest';

import { VAULT_TAB, useApp, type TerminalTab } from './app';

function term(id: string, extra: Partial<TerminalTab> = {}): TerminalTab {
  return {
    id,
    kind: 'terminal',
    title: id,
    hostId: null,
    protocol: 'local',
    sessionId: `s-${id}`,
    status: 'connected',
    unread: false,
    ...extra,
  };
}

describe('tab store', () => {
  beforeEach(() => {
    useApp.setState({ tabs: [], activeTab: VAULT_TAB, closedTabs: [], renaming: null });
  });

  it('reopens closed tabs newest first, as new connections', () => {
    const s = useApp.getState();
    s.openTab(term('a'));
    s.openTab(term('b', { cwd: '/srv' }));
    s.closeTab('b');
    s.closeTab('a');
    expect(useApp.getState().tabs).toEqual([]);
    expect(useApp.getState().activeTab).toBe(VAULT_TAB);
    expect(useApp.getState().reopenClosed()).toBe(true);
    let tabs = useApp.getState().tabs;
    expect(tabs[0]?.title).toBe('a');
    expect(tabs[0]?.id).not.toBe('a');
    expect(tabs[0]?.kind === 'terminal' && tabs[0].status).toBe('connecting');
    expect(useApp.getState().reopenClosed()).toBe(true);
    tabs = useApp.getState().tabs;
    expect(tabs[1]?.kind === 'terminal' && tabs[1].cwd).toBe('/srv');
    expect(useApp.getState().reopenClosed()).toBe(false);
  });

  it('activates the tab to the left when the active tab closes', () => {
    const s = useApp.getState();
    s.openTab(term('a'));
    s.openTab(term('b'));
    s.openTab(term('c'));
    s.activate('b');
    s.closeTab('b');
    expect(useApp.getState().activeTab).toBe('a');
    s.activate('a');
    s.closeTab('a');
    expect(useApp.getState().activeTab).toBe('c');
  });

  it('closes a workspace with its panes', () => {
    const s = useApp.getState();
    s.openTab(term('p1', { workspace: 'w' }));
    s.openTab(term('p2', { workspace: 'w' }));
    s.openTab({
      id: 'w',
      kind: 'workspace',
      title: 'W',
      panes: ['p1', 'p2'],
      mode: 'split',
      broadcast: false,
      focused: 'p1',
      templateId: null,
    });
    s.closeTab('w');
    expect(useApp.getState().tabs).toEqual([]);
    // Workspace panes are not offered for reopening one by one.
    expect(useApp.getState().closedTabs).toEqual([]);
  });

  it('duplicates next to the original and renames', () => {
    const s = useApp.getState();
    s.openTab(term('a'));
    s.openTab(term('b'));
    s.duplicateTab('a', 'a2', '/tmp');
    const tabs = useApp.getState().tabs;
    expect(tabs.map((t) => t.id)).toEqual(['a', 'a2', 'b']);
    expect(useApp.getState().activeTab).toBe('a2');
    s.renameTab('a2', '  logs  ');
    const renamed = useApp.getState().tabs[1];
    expect(renamed?.title).toBe('logs');
    expect(renamed?.kind === 'terminal' && renamed.customTitle).toBe(true);
    s.renameTab('a2', '');
    const reset = useApp.getState().tabs[1];
    expect(reset?.kind === 'terminal' && reset.customTitle).toBe(false);
  });

  it('closes other tabs and tabs to the right', () => {
    const s = useApp.getState();
    for (const id of ['a', 'b', 'c', 'd']) s.openTab(term(id));
    s.closeToRight('b');
    expect(useApp.getState().tabs.map((t) => t.id)).toEqual(['a', 'b']);
    s.closeOthers('b');
    expect(useApp.getState().tabs.map((t) => t.id)).toEqual(['b']);
    expect(useApp.getState().activeTab).toBe('b');
  });

  it('splits a tab into panes and turns back into a tab', () => {
    const s = useApp.getState();
    s.openTab(term('a'));
    s.openTab(term('b'));
    expect(s.splitPane('a', 'a2', 'w', '/srv')).toBe('a2');
    let st = useApp.getState();
    expect(st.activeTab).toBe('w');
    // The workspace takes the tab's place; panes are hidden from the tab bar.
    expect(st.tabs.map((t) => t.id)).toEqual(['w', 'a', 'b', 'a2']);
    const ws = st.tabs[0];
    expect(ws?.kind === 'workspace' && ws.panes).toEqual(['a', 'a2']);
    expect(ws?.kind === 'workspace' && ws.focused).toBe('a2');
    const pane = st.tabs.find((t) => t.id === 'a2');
    expect(pane?.kind === 'terminal' && pane.cwd).toBe('/srv');
    // Splitting the workspace adds a pane like the focused one.
    s.splitPane('w', 'a3', 'unused');
    st = useApp.getState();
    expect(st.tabs[0]?.kind === 'workspace' && st.tabs[0].panes).toEqual(['a', 'a2', 'a3']);
    s.cyclePane('w', 1);
    expect(
      useApp.getState().tabs[0]?.kind === 'workspace' && useApp.getState().tabs[0],
    ).toMatchObject({
      focused: 'a',
    });
    s.closePane('w', 'a3');
    s.closePane('w', 'a');
    st = useApp.getState();
    expect(st.tabs.map((t) => t.id)).toEqual(['a2', 'b']);
    const back = st.tabs[0];
    expect(back?.kind === 'terminal' && back.workspace).toBeUndefined();
    expect(st.activeTab).toBe('a2');
    expect(st.closedTabs.map((t) => t.title)).toEqual(['a', 'a']);
  });

  it('clears finished activity when a tab is shown, but not a running command', () => {
    const s = useApp.getState();
    s.openTab(term('a', { activity: 'failed' }));
    s.openTab(term('b', { activity: 'running' }));
    s.activate('a');
    s.activate('b');
    const [a, b] = useApp.getState().tabs;
    expect(a?.kind === 'terminal' && a.activity).toBeUndefined();
    expect(b?.kind === 'terminal' && b.activity).toBe('running');
  });
});
