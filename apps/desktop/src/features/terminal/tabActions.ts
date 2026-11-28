// SPDX-License-Identifier: GPL-3.0-or-later
/** Tab actions shared by keyboard shortcuts, the tab context menu and the command palette. */
import { newId } from '@/lib/ipc';
import { SFTP_TAB, VAULT_TAB, useApp, type Tab, type TerminalTab } from '@/stores/app';

import { terminalRegistry } from './registry';

/** Tabs shown in the tab bar, in order (workspace panes are inside their workspace). */
export function visibleTabs(tabs: Tab[] = useApp.getState().tabs): Tab[] {
  return tabs.filter((t) => t.kind !== 'sftp' && !(t.kind === 'terminal' && t.workspace));
}

/** The terminal the user is looking at: the active tab, or the focused workspace pane. */
export function activeTerminal(): TerminalTab | null {
  const { tabs, activeTab } = useApp.getState();
  const tab = tabs.find((t) => t.id === activeTab);
  if (tab?.kind === 'terminal') return tab;
  if (tab?.kind === 'workspace') {
    const pane = tabs.find((t) => t.id === (tab.focused ?? tab.panes[0]));
    return pane?.kind === 'terminal' ? pane : null;
  }
  return null;
}

/** The working directory a local terminal tab last reported (OSC 7), if any. */
function localCwd(tab: TerminalTab | null): string | undefined {
  if (tab?.protocol !== 'local') return undefined;
  return terminalRegistry.get(tab.id)?.cwd() ?? tab.cwd;
}

/** Opens a local terminal tab, in the directory of the current local tab if there is one. */
export function openLocalTerminal(cwd?: string): void {
  const id = newId();
  useApp.getState().openTab({
    id,
    kind: 'terminal',
    title: 'Local',
    hostId: null,
    protocol: 'local',
    sessionId: null,
    status: 'connecting',
    unread: false,
    cwd: cwd ?? localCwd(activeTerminal()),
  });
}

/**
 * "New tab": a local terminal where this device has one (like any terminal app), otherwise
 * the connection picker.
 */
export function newTab(localTerminal: boolean): void {
  if (localTerminal) openLocalTerminal();
  else useApp.getState().setNewTabOpen(true);
}

/** Opens another connection like terminal tab `id` next to it. */
export function duplicate(id: string): void {
  const s = useApp.getState();
  const tab = s.tabs.find((t) => t.id === id);
  if (tab?.kind !== 'terminal') return;
  s.duplicateTab(id, newId(), localCwd(tab));
}

/** The tab that tab-level actions (close, rename, duplicate) apply to. */
export function currentTabId(): string | null {
  const { activeTab } = useApp.getState();
  return activeTab === VAULT_TAB || activeTab === SFTP_TAB ? null : activeTab;
}

/** Activates the tab `delta` steps away in the tab bar (Vault and SFTP included), wrapping. */
export function cycleTab(delta: number): void {
  const s = useApp.getState();
  const ids = [VAULT_TAB, SFTP_TAB, ...visibleTabs(s.tabs).map((t) => t.id)];
  const i = ids.indexOf(s.activeTab);
  const next = ids[(i + delta + ids.length * 2) % ids.length];
  if (next) s.activate(next);
}

/** Activates the n-th tab (1-based; 9 is always the last tab, as in browsers). */
export function goToTab(n: number): void {
  const s = useApp.getState();
  const ids = [VAULT_TAB, SFTP_TAB, ...visibleTabs(s.tabs).map((t) => t.id)];
  const target = n === 9 ? ids[ids.length - 1] : ids[n - 1];
  if (target) s.activate(target);
}

/** Moves the active session tab one place left or right. */
export function moveActive(delta: -1 | 1): void {
  const s = useApp.getState();
  const visible = visibleTabs(s.tabs);
  const i = visible.findIndex((t) => t.id === s.activeTab);
  const target = visible[i + delta];
  const self = visible[i];
  if (!self || !target) return;
  s.moveTab(s.tabs.indexOf(self), s.tabs.indexOf(target));
}
