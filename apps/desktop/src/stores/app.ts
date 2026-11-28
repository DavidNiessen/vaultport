// SPDX-License-Identifier: GPL-3.0-or-later
/** UI state: tabs, navigation, selection, panels. Data lives in the Rust core, not here. */
import { create } from 'zustand';
import { persist } from 'zustand/middleware';

import { DEFAULT_PRESET, isPreset, type PresetId } from '@/lib/presets';

export type VaultSection =
  'hosts' | 'keychain' | 'forwarding' | 'snippets' | 'knownHosts' | 'logs' | 'settings';

export type ThemePref = 'system' | 'dark' | 'light';

export type SessionStatus = 'connecting' | 'connected' | 'disconnected' | 'error';

export interface TerminalTab {
  id: string;
  kind: 'terminal';
  title: string;
  hostId: string | null;
  protocol: 'ssh' | 'mosh' | 'telnet' | 'local' | 'serial';
  /** Core session id once connected. */
  sessionId: string | null;
  status: SessionStatus;
  /** Unread output since the tab was last focused. */
  unread: boolean;
  quick?: { host: string; port: number | null; username: string | null } | undefined;
  /** Text sent once after the session connects (e.g. a snippet). */
  initialInput?: string | undefined;
  serial?:
    | {
        path: string;
        baudRate: number;
        dataBits: number;
        stopBits: number;
        parity: string;
        flowControl: string;
      }
    | undefined;
  /** Workspace the terminal belongs to, if any. */
  workspace?: string | undefined;
  /** Working directory to start a local shell in (e.g. inherited from another tab). */
  cwd?: string | undefined;
  /** The user renamed the tab: remote programs may no longer change its title. */
  customTitle?: boolean | undefined;
}

export interface SftpTab {
  id: string;
  kind: 'sftp';
  title: string;
}

export interface WorkspaceTab {
  id: string;
  kind: 'workspace';
  title: string;
  panes: string[];
  mode: 'split' | 'focus';
  broadcast: boolean;
  focused: string | null;
  templateId: string | null;
}

export interface LogTab {
  id: string;
  kind: 'log';
  title: string;
  logId: string;
}

export type Tab = TerminalTab | SftpTab | WorkspaceTab | LogTab;

export const VAULT_TAB = 'vault';
export const SFTP_TAB = 'sftp';

/** Closed terminal tabs remembered for "Reopen closed tab". */
const MAX_CLOSED = 20;

/** A fresh, unconnected copy of a terminal tab (for duplicate and reopen). */
export function cloneTerminal(tab: TerminalTab, id: string, cwd?: string): TerminalTab {
  return {
    id,
    kind: 'terminal',
    title: tab.title,
    hostId: tab.hostId,
    protocol: tab.protocol,
    sessionId: null,
    status: 'connecting',
    unread: false,
    quick: tab.quick,
    serial: tab.serial,
    cwd: cwd ?? tab.cwd,
    customTitle: tab.customTitle,
  };
}

interface AppState {
  theme: ThemePref;
  preset: PresetId;
  /** Slowly moving glow behind the UI (also off with prefers-reduced-motion). */
  animatedBackground: boolean;
  section: VaultSection;
  selected: { kind: 'host' | 'group'; id: string } | null;
  currentGroup: string | null;
  hostView: 'grid' | 'list';
  tagFilter: string[];
  tabs: Tab[];
  activeTab: string;
  paletteOpen: boolean;
  sidePanelOpen: boolean;
  newTabOpen: boolean;
  /** Recently closed terminal tabs, newest last. */
  closedTabs: TerminalTab[];
  /** Tab whose title is being edited in the tab bar. */
  renaming: string | null;

  setTheme: (t: ThemePref) => void;
  setPreset: (p: PresetId) => void;
  setAnimatedBackground: (on: boolean) => void;
  setSection: (s: VaultSection) => void;
  select: (s: AppState['selected']) => void;
  setCurrentGroup: (g: string | null) => void;
  setHostView: (v: 'grid' | 'list') => void;
  setTagFilter: (t: string[]) => void;
  openTab: (tab: Tab) => void;
  updateTab: (id: string, patch: Partial<Tab>) => void;
  closeTab: (id: string) => void;
  /** Closes every session tab except `id`. */
  closeOthers: (id: string) => void;
  /** Closes the session tabs to the right of `id`. */
  closeToRight: (id: string) => void;
  /** Reopens the most recently closed terminal tab. Returns `false` if there is none. */
  reopenClosed: () => boolean;
  /** Opens a new connection like terminal tab `id`, right next to it. */
  duplicateTab: (id: string, newId: string, cwd?: string) => void;
  /** Renames a tab (an empty title gives control back to the remote program). */
  renameTab: (id: string, title: string) => void;
  /**
   * Splits the tab `id` (or the focused pane of a workspace): the tab becomes a workspace with
   * a second connection like it. Returns the new pane's id.
   */
  splitPane: (id: string, newId: string, wsId: string, cwd?: string) => string | null;
  /** Closes one pane of a workspace; a workspace left with one pane turns back into a tab. */
  closePane: (wsId: string, paneId: string) => void;
  /** Focuses the next / previous pane of a workspace. */
  cyclePane: (wsId: string, delta: number) => void;
  setRenaming: (id: string | null) => void;
  activate: (id: string) => void;
  moveTab: (from: number, to: number) => void;
  setPaletteOpen: (o: boolean) => void;
  setSidePanelOpen: (o: boolean) => void;
  setNewTabOpen: (o: boolean) => void;
}

export const useApp = create<AppState>()(
  persist(
    (set, get) => ({
      theme: 'system',
      preset: DEFAULT_PRESET,
      animatedBackground: true,
      section: 'hosts',
      selected: null,
      currentGroup: null,
      hostView: 'grid',
      tagFilter: [],
      tabs: [],
      activeTab: VAULT_TAB,
      paletteOpen: false,
      sidePanelOpen: false,
      newTabOpen: false,
      closedTabs: [],
      renaming: null,

      setTheme: (theme) => set({ theme }),
      setPreset: (preset) => set({ preset }),
      setAnimatedBackground: (animatedBackground) => set({ animatedBackground }),
      setSection: (section) => set({ section, selected: null, activeTab: VAULT_TAB }),
      select: (selected) => set({ selected }),
      setCurrentGroup: (currentGroup) => set({ currentGroup, selected: null }),
      setHostView: (hostView) => set({ hostView }),
      setTagFilter: (tagFilter) => set({ tagFilter }),
      openTab: (tab) =>
        set((s) => ({
          tabs: s.tabs.some((t) => t.id === tab.id) ? s.tabs : [...s.tabs, tab],
          activeTab: tab.id,
          newTabOpen: false,
        })),
      updateTab: (id, patch) =>
        set((s) => ({
          tabs: s.tabs.map((t) => (t.id === id ? ({ ...t, ...patch } as Tab) : t)),
        })),
      closeTab: (id) => {
        const { tabs, activeTab } = get();
        const idx = tabs.findIndex((t) => t.id === id);
        const closing = tabs[idx];
        if (!closing) return;
        // A workspace takes its panes with it.
        const gone = new Set([id, ...(closing.kind === 'workspace' ? closing.panes : [])]);
        const next = tabs.filter((t) => !gone.has(t.id));
        let active = activeTab;
        if (gone.has(activeTab)) {
          const visible = next.filter((t) => !(t.kind === 'terminal' && t.workspace));
          const before = tabs
            .slice(0, idx)
            .filter((t) => !(t.kind === 'terminal' && t.workspace) && !gone.has(t.id));
          active = before[before.length - 1]?.id ?? visible[0]?.id ?? VAULT_TAB;
        }
        const remembered =
          closing.kind === 'terminal' && !closing.workspace
            ? [...get().closedTabs, closing].slice(-MAX_CLOSED)
            : get().closedTabs;
        set({ tabs: next, activeTab: active, closedTabs: remembered });
      },
      closeOthers: (id) => {
        for (const t of get().tabs) {
          if (t.id !== id && !(t.kind === 'terminal' && t.workspace)) get().closeTab(t.id);
        }
        get().activate(id);
      },
      closeToRight: (id) => {
        const visible = get().tabs.filter((t) => !(t.kind === 'terminal' && t.workspace));
        const i = visible.findIndex((t) => t.id === id);
        if (i < 0) return;
        for (const t of visible.slice(i + 1)) get().closeTab(t.id);
      },
      reopenClosed: () => {
        const closed = [...get().closedTabs];
        const last = closed.pop();
        if (!last) return false;
        const tab = cloneTerminal(last, crypto.randomUUID());
        set((s) => ({ closedTabs: closed, tabs: [...s.tabs, tab], activeTab: tab.id }));
        return true;
      },
      duplicateTab: (id, newId, cwd) => {
        const { tabs } = get();
        const i = tabs.findIndex((t) => t.id === id);
        const tab = tabs[i];
        if (tab?.kind !== 'terminal') return;
        const copy = cloneTerminal({ ...tab, workspace: undefined }, newId, cwd);
        const next = [...tabs];
        next.splice(i + 1, 0, copy);
        set({ tabs: next, activeTab: newId });
      },
      renameTab: (id, title) =>
        set((s) => ({
          renaming: null,
          tabs: s.tabs.map((t) => {
            if (t.id !== id) return t;
            const trimmed = title.trim();
            if (t.kind !== 'terminal') return trimmed ? { ...t, title: trimmed } : t;
            return trimmed
              ? { ...t, title: trimmed, customTitle: true }
              : { ...t, customTitle: false };
          }),
        })),
      setRenaming: (renaming) => set({ renaming }),
      splitPane: (id, newId, wsId, cwd) => {
        const { tabs } = get();
        const target = tabs.find((t) => t.id === id);
        if (target?.kind === 'workspace') {
          const src = tabs.find((t) => t.id === (target.focused ?? target.panes[0]));
          if (src?.kind !== 'terminal' || target.panes.length >= 16) return null;
          const pane = { ...cloneTerminal(src, newId, cwd), workspace: target.id };
          set({
            tabs: [
              ...tabs.map((t) =>
                t.id === target.id
                  ? {
                      ...target,
                      panes: [...target.panes, newId],
                      focused: newId,
                      mode: 'split' as const,
                    }
                  : t,
              ),
              pane,
            ],
            activeTab: target.id,
          });
          return newId;
        }
        if (target?.kind !== 'terminal' || target.workspace) return null;
        const ws: WorkspaceTab = {
          id: wsId,
          kind: 'workspace',
          title: target.title,
          panes: [target.id, newId],
          mode: 'split',
          broadcast: false,
          focused: newId,
          templateId: null,
        };
        const pane = { ...cloneTerminal(target, newId, cwd), workspace: wsId };
        set({
          // The workspace takes the tab's place in the tab bar.
          tabs: [
            ...tabs.flatMap((t) => (t.id === id ? [ws, { ...target, workspace: wsId }] : [t])),
            pane,
          ],
          activeTab: wsId,
        });
        return newId;
      },
      closePane: (wsId, paneId) => {
        const { tabs, activeTab } = get();
        const ws = tabs.find((t) => t.id === wsId);
        if (ws?.kind !== 'workspace' || !ws.panes.includes(paneId)) return;
        const closing = tabs.find((t) => t.id === paneId);
        const panes = ws.panes.filter((p) => p !== paneId);
        const closed =
          closing?.kind === 'terminal'
            ? [...get().closedTabs, { ...closing, workspace: undefined }].slice(-MAX_CLOSED)
            : get().closedTabs;
        if (panes.length === 0) {
          set({ closedTabs: closed });
          get().closeTab(wsId);
          return;
        }
        if (panes.length === 1) {
          // Back to a plain tab, in the workspace's place.
          const last = panes[0];
          const rest = tabs.filter((t) => t.id !== paneId && t.id !== last);
          const lastTab = tabs.find((t) => t.id === last);
          if (lastTab?.kind !== 'terminal') return;
          const plain: TerminalTab = { ...lastTab, workspace: undefined };
          set({
            tabs: rest.map((t) => (t.id === wsId ? plain : t)),
            activeTab: activeTab === wsId ? plain.id : activeTab,
            closedTabs: closed,
          });
          return;
        }
        const i = ws.panes.indexOf(paneId);
        set({
          tabs: tabs
            .filter((t) => t.id !== paneId)
            .map((t) =>
              t.id === wsId
                ? { ...ws, panes, focused: panes[Math.min(i, panes.length - 1)] ?? null }
                : t,
            ),
          closedTabs: closed,
        });
      },
      cyclePane: (wsId, delta) => {
        const ws = get().tabs.find((t) => t.id === wsId);
        if (ws?.kind !== 'workspace' || ws.panes.length === 0) return;
        const i = ws.panes.indexOf(ws.focused ?? ws.panes[0] ?? '');
        const next = ws.panes[(i + delta + ws.panes.length) % ws.panes.length] ?? null;
        get().updateTab(wsId, { focused: next });
      },
      activate: (activeTab) =>
        set((s) => ({
          activeTab,
          tabs: s.tabs.map((t) =>
            t.id === activeTab && t.kind === 'terminal' ? { ...t, unread: false } : t,
          ),
        })),
      moveTab: (from, to) =>
        set((s) => {
          const tabs = [...s.tabs];
          const [moved] = tabs.splice(from, 1);
          if (moved) tabs.splice(to, 0, moved);
          return { tabs };
        }),
      setPaletteOpen: (paletteOpen) => set({ paletteOpen }),
      setSidePanelOpen: (sidePanelOpen) => set({ sidePanelOpen }),
      setNewTabOpen: (newTabOpen) => set({ newTabOpen }),
    }),
    {
      name: 'vaultport:ui',
      version: 2,
      // Version 1 stored a free accent colour; colour presets replaced it.
      migrate: (persisted) => {
        const state = { ...(persisted as Partial<AppState> & { accent?: string }) };
        delete state.accent;
        return {
          ...state,
          preset: isPreset(state.preset) ? state.preset : DEFAULT_PRESET,
        } as AppState;
      },
      // Only persist harmless view preferences; never tabs/sessions.
      partialize: (s) => ({
        theme: s.theme,
        preset: s.preset,
        animatedBackground: s.animatedBackground,
        hostView: s.hostView,
        section: s.section,
      }),
    },
  ),
);
