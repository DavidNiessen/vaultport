// SPDX-License-Identifier: GPL-3.0-or-later
/**
 * Keyboard shortcuts.
 *
 * A binding is a string such as `Mod+Shift+T`: modifiers (`Mod`, `Ctrl`, `Alt`, `Shift`)
 * followed by one key. `Mod` is ⌘ on Apple platforms and Ctrl elsewhere; `Ctrl` is the
 * Control key everywhere. Keys are either a printable character (`T`, `,`, `[`, `` ` ``) or a
 * named key (`Tab`, `Up`, `PageDown`, `Space`, `F2`, …).
 *
 * Every action has one or more default bindings (which may differ between Apple platforms and
 * the rest) and the user can replace them in Settings → Shortcuts; overrides are stored in the
 * vault settings document `shortcuts`.
 */
import { create } from 'zustand';

export type ShortcutGroup = 'tabs' | 'app' | 'terminal' | 'workspaces';

/**
 * `global` shortcuts work everywhere; `terminal` ones only while a terminal has focus (they
 * are handled by the terminal so the key is not also sent to the shell).
 */
export type ShortcutScope = 'global' | 'terminal';

export interface Shortcut {
  id: ShortcutId;
  /** i18n key of the action name. */
  label: string;
  group: ShortcutGroup;
  scope: ShortcutScope;
  /** Default bindings on Apple platforms. */
  apple: string[];
  /** Default bindings elsewhere. */
  other: string[];
}

export type ShortcutId =
  | 'newTab'
  | 'connect'
  | 'closeTab'
  | 'reopenTab'
  | 'duplicateTab'
  | 'renameTab'
  | 'nextTab'
  | 'prevTab'
  | 'moveTabLeft'
  | 'moveTabRight'
  | 'palette'
  | 'jump'
  | 'local'
  | 'sidePanel'
  | 'settings'
  | 'lock'
  | 'search'
  | 'clear'
  | 'historySearch'
  | 'suggest'
  | 'aiCommand'
  | 'composer'
  | 'prevBlock'
  | 'nextBlock'
  | 'copyLastOutput'
  | 'zoomIn'
  | 'zoomOut'
  | 'zoomReset'
  | 'paste'
  | 'broadcast'
  | 'splitMode';

const both = (...keys: string[]) => ({ apple: keys, other: keys });

export const SHORTCUTS: Shortcut[] = [
  // Tabs
  { id: 'newTab', label: 'shortcuts.newTab', group: 'tabs', scope: 'global', ...both('Mod+T') },
  {
    id: 'connect',
    label: 'shortcuts.connect',
    group: 'tabs',
    scope: 'global',
    ...both('Mod+O', 'Mod+Shift+N'),
  },
  {
    id: 'closeTab',
    label: 'shortcuts.closeTab',
    group: 'tabs',
    scope: 'global',
    ...both('Mod+W', 'Mod+Shift+W'),
  },
  {
    id: 'reopenTab',
    label: 'shortcuts.reopenTab',
    group: 'tabs',
    scope: 'global',
    ...both('Mod+Shift+T'),
  },
  {
    id: 'duplicateTab',
    label: 'shortcuts.duplicateTab',
    group: 'tabs',
    scope: 'global',
    ...both('Mod+Shift+D'),
  },
  {
    id: 'renameTab',
    label: 'shortcuts.renameTab',
    group: 'tabs',
    scope: 'global',
    ...both('Mod+Shift+R'),
  },
  {
    id: 'nextTab',
    label: 'shortcuts.nextTab',
    group: 'tabs',
    scope: 'global',
    apple: ['Ctrl+Tab', 'Mod+Shift+]', 'Mod+Alt+Right'],
    other: ['Ctrl+Tab', 'Ctrl+PageDown'],
  },
  {
    id: 'prevTab',
    label: 'shortcuts.prevTab',
    group: 'tabs',
    scope: 'global',
    apple: ['Ctrl+Shift+Tab', 'Mod+Shift+[', 'Mod+Alt+Left'],
    other: ['Ctrl+Shift+Tab', 'Ctrl+PageUp'],
  },
  {
    id: 'moveTabLeft',
    label: 'shortcuts.moveTabLeft',
    group: 'tabs',
    scope: 'global',
    apple: ['Mod+Shift+Alt+Left'],
    other: ['Ctrl+Shift+PageUp'],
  },
  {
    id: 'moveTabRight',
    label: 'shortcuts.moveTabRight',
    group: 'tabs',
    scope: 'global',
    apple: ['Mod+Shift+Alt+Right'],
    other: ['Ctrl+Shift+PageDown'],
  },
  // App
  {
    id: 'palette',
    label: 'shortcuts.palette',
    group: 'app',
    scope: 'global',
    ...both('Mod+K', 'Mod+Shift+P'),
  },
  { id: 'jump', label: 'shortcuts.jump', group: 'app', scope: 'global', ...both('Mod+J') },
  // Mod+T opens a local terminal already, and Ctrl+L is "clear screen" in shells.
  { id: 'local', label: 'shortcuts.local', group: 'app', scope: 'global', ...both() },
  {
    id: 'sidePanel',
    label: 'shortcuts.sidePanel',
    group: 'app',
    scope: 'global',
    ...both('Mod+.'),
  },
  { id: 'settings', label: 'shortcuts.settings', group: 'app', scope: 'global', ...both('Mod+,') },
  { id: 'lock', label: 'shortcuts.lock', group: 'app', scope: 'global', ...both('Mod+Shift+L') },
  // Terminal
  {
    id: 'search',
    label: 'shortcuts.search',
    group: 'terminal',
    scope: 'terminal',
    apple: ['Mod+F', 'Mod+Shift+F'],
    other: ['Ctrl+Shift+F'],
  },
  {
    id: 'clear',
    label: 'shortcuts.clear',
    group: 'terminal',
    scope: 'terminal',
    ...both('Mod+Shift+K'),
  },
  {
    id: 'historySearch',
    label: 'shortcuts.historySearch',
    group: 'terminal',
    scope: 'terminal',
    ...both('Ctrl+R'),
  },
  {
    id: 'suggest',
    label: 'shortcuts.suggest',
    group: 'terminal',
    scope: 'terminal',
    ...both('Ctrl+Space'),
  },
  {
    id: 'aiCommand',
    label: 'shortcuts.aiCommand',
    group: 'terminal',
    scope: 'terminal',
    ...both('Ctrl+`'),
  },
  {
    id: 'composer',
    label: 'shortcuts.composer',
    group: 'terminal',
    scope: 'terminal',
    ...both('Mod+Shift+E'),
  },
  {
    id: 'prevBlock',
    label: 'shortcuts.prevBlock',
    group: 'terminal',
    scope: 'terminal',
    apple: ['Mod+Up'],
    other: ['Ctrl+Shift+Up'],
  },
  {
    id: 'nextBlock',
    label: 'shortcuts.nextBlock',
    group: 'terminal',
    scope: 'terminal',
    apple: ['Mod+Down'],
    other: ['Ctrl+Shift+Down'],
  },
  {
    id: 'copyLastOutput',
    label: 'shortcuts.copyLastOutput',
    group: 'terminal',
    scope: 'terminal',
    ...both('Mod+Shift+O'),
  },
  {
    id: 'zoomIn',
    label: 'shortcuts.zoomIn',
    group: 'terminal',
    scope: 'terminal',
    ...both('Mod+=', 'Mod++', 'Mod+Shift+='),
  },
  {
    id: 'zoomOut',
    label: 'shortcuts.zoomOut',
    group: 'terminal',
    scope: 'terminal',
    ...both('Mod+-'),
  },
  {
    id: 'zoomReset',
    label: 'shortcuts.zoomReset',
    group: 'terminal',
    scope: 'terminal',
    ...both('Mod+0'),
  },
  {
    id: 'paste',
    label: 'shortcuts.paste',
    group: 'terminal',
    scope: 'terminal',
    apple: [],
    other: ['Ctrl+Shift+V'],
  },
  // Workspaces
  {
    id: 'broadcast',
    label: 'shortcuts.broadcast',
    group: 'workspaces',
    scope: 'global',
    ...both('Mod+Shift+B'),
  },
  {
    id: 'splitMode',
    label: 'shortcuts.splitMode',
    group: 'workspaces',
    scope: 'global',
    ...both('Mod+Shift+M'),
  },
];

export const SHORTCUT_GROUPS: ShortcutGroup[] = ['tabs', 'app', 'terminal', 'workspaces'];

/** User overrides: action id → bindings (an empty list disables the action). */
export type ShortcutOverrides = Partial<Record<ShortcutId, string[]>>;

export const SHORTCUTS_DOC = 'shortcuts';

/** Bindings for every action on this platform, with the user's overrides applied. */
export type Keymap = Record<ShortcutId, string[]>;

export function buildKeymap(apple: boolean, overrides: ShortcutOverrides = {}): Keymap {
  const map = {} as Keymap;
  for (const s of SHORTCUTS) {
    const custom = overrides[s.id];
    map[s.id] = Array.isArray(custom)
      ? custom.filter((k) => typeof k === 'string' && parseBinding(k) !== null)
      : apple
        ? s.apple
        : s.other;
  }
  return map;
}

interface KeymapState {
  apple: boolean;
  keymap: Keymap;
  overrides: ShortcutOverrides;
  /** A shortcut is being recorded in Settings: no action runs. */
  recording: boolean;
  set: (apple: boolean, overrides: ShortcutOverrides) => void;
  setRecording: (recording: boolean) => void;
}

/** The live keymap, kept in sync with the settings document by the app shell. */
export const useKeymap = create<KeymapState>()((set) => ({
  apple: false,
  keymap: buildKeymap(false),
  overrides: {},
  recording: false,
  set: (apple, overrides) => set({ apple, overrides, keymap: buildKeymap(apple, overrides) }),
  setRecording: (recording) => set({ recording }),
}));

interface Binding {
  mod: boolean;
  ctrl: boolean;
  alt: boolean;
  shift: boolean;
  /** Normalized key (see `normalizeKey`). */
  key: string;
}

const MODIFIERS = new Set(['mod', 'ctrl', 'alt', 'shift']);

const NAMED: Record<string, string> = {
  up: 'arrowup',
  down: 'arrowdown',
  left: 'arrowleft',
  right: 'arrowright',
  arrowup: 'arrowup',
  arrowdown: 'arrowdown',
  arrowleft: 'arrowleft',
  arrowright: 'arrowright',
  esc: 'escape',
  escape: 'escape',
  space: ' ',
  enter: 'enter',
  return: 'enter',
  tab: 'tab',
  backspace: 'backspace',
  delete: 'delete',
  del: 'delete',
  home: 'home',
  end: 'end',
  pageup: 'pageup',
  pagedown: 'pagedown',
  insert: 'insert',
  plus: '+',
};

function normalizeKey(k: string): string {
  const lower = k.toLowerCase();
  return NAMED[lower] ?? lower;
}

/** Parses a binding string; `null` if it is malformed. */
export function parseBinding(keys: string): Binding | null {
  // `Mod++` ends in the "+" key.
  const parts = keys.endsWith('++') ? [...keys.slice(0, -2).split('+'), '+'] : keys.split('+');
  const key = parts.pop();
  if (!key) return null;
  const mods = parts.map((p) => p.toLowerCase());
  if (mods.some((m) => !MODIFIERS.has(m))) return null;
  const k = normalizeKey(key);
  if (MODIFIERS.has(k)) return null;
  return {
    mod: mods.includes('mod'),
    ctrl: mods.includes('ctrl'),
    alt: mods.includes('alt'),
    shift: mods.includes('shift'),
    key: k,
  };
}

/** Keys the event could stand for: the produced character and the physical key. */
function eventKeys(e: KeyboardEvent): string[] {
  const keys = [normalizeKey(e.key)];
  const code = e.code;
  if (code.startsWith('Key')) keys.push(code.slice(3).toLowerCase());
  else if (code.startsWith('Digit')) keys.push(code.slice(5));
  else if (code.startsWith('Numpad') && /^\d$/.test(code.slice(6))) keys.push(code.slice(6));
  else {
    const byCode: Record<string, string> = {
      Backquote: '`',
      Minus: '-',
      Equal: '=',
      BracketLeft: '[',
      BracketRight: ']',
      Backslash: '\\',
      Semicolon: ';',
      Quote: "'",
      Comma: ',',
      Period: '.',
      Slash: '/',
      Space: ' ',
      NumpadAdd: '+',
      NumpadSubtract: '-',
    };
    const k = byCode[code];
    if (k) keys.push(k);
  }
  return keys;
}

/** Whether `e` matches the binding `keys` on an Apple (`apple`) or other platform. */
export function matches(e: KeyboardEvent, keys: string, apple: boolean): boolean {
  const b = parseBinding(keys);
  if (!b) return false;
  // On Apple platforms Mod is ⌘ and Ctrl is ⌃; elsewhere both mean Control.
  const wantMeta = apple && b.mod;
  const wantCtrl = apple ? b.ctrl : b.mod || b.ctrl;
  if (e.metaKey !== wantMeta || e.ctrlKey !== wantCtrl || e.altKey !== b.alt) return false;
  const keysOf = eventKeys(e);
  // Shift changes the produced character ("+" is Shift+= on many layouts), so a binding
  // without Shift that names a shifted symbol still matches.
  if (e.shiftKey !== b.shift) {
    const shiftedSymbol = e.shiftKey && !b.shift && !/^[a-z0-9]$/.test(b.key) && b.key.length === 1;
    if (!shiftedSymbol || normalizeKey(e.key) !== b.key) return false;
  }
  return keysOf.includes(b.key);
}

/** The action bound to `e` in `keymap`, limited to `scope`. */
export function actionFor(
  e: KeyboardEvent,
  keymap: Keymap,
  apple: boolean,
  scope: ShortcutScope,
): ShortcutId | null {
  for (const s of SHORTCUTS) {
    if (s.scope !== scope) continue;
    if (keymap[s.id].some((k) => matches(e, k, apple))) return s.id;
  }
  return null;
}

/** The binding string for a key press (for recording a new shortcut), or `null` while only
 * modifiers are held. */
export function bindingFromEvent(e: KeyboardEvent, apple: boolean): string | null {
  const k = e.key;
  if (['Control', 'Meta', 'Alt', 'Shift', 'OS', 'AltGraph', 'CapsLock', 'Dead'].includes(k)) {
    return null;
  }
  const parts: string[] = [];
  if (apple ? e.metaKey : e.ctrlKey) parts.push('Mod');
  if (apple && e.ctrlKey) parts.push('Ctrl');
  if (e.altKey) parts.push('Alt');
  if (e.shiftKey) parts.push('Shift');
  let key: string;
  if (e.code.startsWith('Key')) key = e.code.slice(3);
  else if (e.code.startsWith('Digit')) key = e.code.slice(5);
  else {
    const named: Record<string, string> = {
      ArrowUp: 'Up',
      ArrowDown: 'Down',
      ArrowLeft: 'Left',
      ArrowRight: 'Right',
      ' ': 'Space',
      Escape: 'Esc',
    };
    key = named[k] ?? (k.length === 1 ? k.toUpperCase() : k);
  }
  parts.push(key);
  return parts.join('+');
}

/** The first binding of `id` (for menus and tooltips), if any. */
export function primaryBinding(keymap: Keymap, id: ShortcutId): string | undefined {
  return keymap[id][0];
}

/** Which actions share a binding with another action in the same or global scope. */
export function conflicts(keymap: Keymap): Map<string, ShortcutId[]> {
  const byKey = new Map<string, ShortcutId[]>();
  for (const s of SHORTCUTS) {
    for (const k of keymap[s.id]) {
      const norm = k.toLowerCase();
      byKey.set(norm, [...(byKey.get(norm) ?? []), s.id]);
    }
  }
  return new Map([...byKey].filter(([, ids]) => ids.length > 1));
}
