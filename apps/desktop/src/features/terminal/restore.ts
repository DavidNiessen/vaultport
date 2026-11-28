// SPDX-License-Identifier: GPL-3.0-or-later
/**
 * Restoring tabs on start (opt-in, Settings → Terminal): the open terminal tabs and split
 * workspaces are saved in the device-only vault settings document `openTabs` (encrypted like
 * the rest of the vault, not synced) and reopened, as new connections, after the next unlock. Only references are
 * saved — host ids, titles and working directories — never credentials or output.
 */
import { useEffect, useRef } from 'react';

import { commands, newId } from '@/lib/ipc';
import { useSettings } from '@/lib/queries';
import { useApp, type Tab, type TerminalTab } from '@/stores/app';

import { terminalRegistry } from './registry';
import { DEFAULT_TERMINAL_SETTINGS, type TerminalSettings } from './settings';

export const OPEN_TABS_DOC = 'openTabs';

type SavedTerminal = Pick<
  TerminalTab,
  'title' | 'hostId' | 'protocol' | 'quick' | 'serial' | 'cwd' | 'customTitle'
>;

export type SavedTab =
  | { kind: 'terminal'; tab: SavedTerminal }
  | { kind: 'workspace'; title: string; panes: SavedTerminal[] };

export interface SavedTabs {
  tabs: SavedTab[];
  /** Index of the active tab in `tabs`, if a session tab was active. */
  active: number | null;
}

function saveTerminal(t: TerminalTab): SavedTerminal {
  return {
    title: t.title,
    hostId: t.hostId,
    protocol: t.protocol,
    quick: t.quick,
    serial: t.serial,
    cwd: t.protocol === 'local' ? (terminalRegistry.get(t.id)?.cwd() ?? t.cwd) : undefined,
    customTitle: t.customTitle,
  };
}

/** What to save for the tabs in `tabs` (log viewers are left out). */
export function snapshot(tabs: Tab[], activeTab: string): SavedTabs {
  const out: SavedTab[] = [];
  let active: number | null = null;
  for (const t of tabs) {
    if (t.kind === 'terminal' && !t.workspace) {
      if (t.id === activeTab) active = out.length;
      out.push({ kind: 'terminal', tab: saveTerminal(t) });
    } else if (t.kind === 'workspace') {
      const panes = t.panes
        .map((id) => tabs.find((x) => x.id === id))
        .filter((x): x is TerminalTab => x?.kind === 'terminal')
        .map(saveTerminal);
      if (panes.length === 0) continue;
      if (t.id === activeTab) active = out.length;
      out.push({ kind: 'workspace', title: t.title, panes });
    }
  }
  return { tabs: out, active };
}

function terminal(saved: SavedTerminal, workspace?: string): TerminalTab {
  return {
    ...saved,
    id: newId(),
    kind: 'terminal',
    sessionId: null,
    status: 'connecting',
    unread: false,
    workspace,
  };
}

/** Tabs (new ids, not connected) for a snapshot, and which one to activate. */
export function restoreTabs(saved: SavedTabs): { tabs: Tab[]; active: string | null } {
  const tabs: Tab[] = [];
  let active: string | null = null;
  saved.tabs.forEach((s, i) => {
    let id: string;
    if (s.kind === 'terminal') {
      const t = terminal(s.tab);
      tabs.push(t);
      id = t.id;
    } else {
      id = newId();
      const panes = s.panes.map((p) => terminal(p, id));
      tabs.push(
        {
          id,
          kind: 'workspace',
          title: s.title,
          panes: panes.map((p) => p.id),
          mode: 'split',
          broadcast: false,
          focused: panes[0]?.id ?? null,
          templateId: null,
        },
        ...panes,
      );
    }
    if (i === saved.active) active = id;
  });
  return { tabs, active };
}

/** Saves the open tabs while "Restore tabs" is on, and reopens them once after unlocking. */
export function useTabRestore(): void {
  const { settings, loading } = useSettings<TerminalSettings>(
    'terminal',
    DEFAULT_TERMINAL_SETTINGS,
  );
  const restored = useRef(false);
  const enabled = settings.restoreTabs;

  // Reopen once, after the settings are known.
  useEffect(() => {
    if (loading || restored.current) return;
    restored.current = true;
    if (!enabled || useApp.getState().tabs.length > 0) return;
    void commands
      .getSettings(OPEN_TABS_DOC)
      .then((v) => {
        const saved = v as SavedTabs | null;
        if (!saved || !Array.isArray(saved.tabs) || saved.tabs.length === 0) return;
        const { tabs, active } = restoreTabs(saved);
        useApp.setState((s) => ({
          tabs: [...s.tabs, ...tabs],
          activeTab: active ?? s.activeTab,
        }));
      })
      .catch(() => undefined);
  }, [loading, enabled]);

  // Save on changes (debounced), and clear the document when the option is turned off.
  useEffect(() => {
    if (loading) return;
    if (!enabled) {
      void commands.saveSettings(OPEN_TABS_DOC, null, true).catch(() => undefined);
      return;
    }
    let timer: ReturnType<typeof setTimeout> | null = null;
    let last = '';
    const save = () => {
      const { tabs, activeTab } = useApp.getState();
      const snap = snapshot(tabs, activeTab);
      const json = JSON.stringify(snap);
      if (json === last) return;
      last = json;
      void commands.saveSettings(OPEN_TABS_DOC, snap, true).catch(() => undefined);
    };
    const unsubscribe = useApp.subscribe((s, prev) => {
      if (s.tabs === prev.tabs && s.activeTab === prev.activeTab) return;
      if (timer) clearTimeout(timer);
      timer = setTimeout(save, 1000);
    });
    // Working directories change without a tab change; pick them up now and then.
    const interval = setInterval(save, 30_000);
    return () => {
      unsubscribe();
      clearInterval(interval);
      if (timer) clearTimeout(timer);
    };
  }, [loading, enabled]);
}
