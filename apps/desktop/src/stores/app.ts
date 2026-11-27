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
        const next = tabs.filter((t) => t.id !== id);
        let active = activeTab;
        if (activeTab === id) {
          active = next[Math.max(0, idx - 1)]?.id ?? VAULT_TAB;
        }
        set({ tabs: next, activeTab: active });
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
