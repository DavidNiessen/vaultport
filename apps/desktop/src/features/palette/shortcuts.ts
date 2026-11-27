// SPDX-License-Identifier: GPL-3.0-or-later
/** Default keyboard shortcuts. `Mod` is ⌘ on Apple platforms and Ctrl elsewhere. */
export interface Shortcut {
  id: string;
  keys: string;
  label: string;
}

export const SHORTCUTS: Shortcut[] = [
  { id: 'newTab', keys: 'Mod+T', label: 'shortcuts.newTab' },
  { id: 'palette', keys: 'Mod+K', label: 'shortcuts.palette' },
  { id: 'jump', keys: 'Mod+J', label: 'shortcuts.jump' },
  { id: 'local', keys: 'Mod+L', label: 'shortcuts.local' },
  { id: 'closeTab', keys: 'Mod+Shift+W', label: 'shortcuts.closeTab' },
  { id: 'nextTab', keys: 'Ctrl+Tab', label: 'shortcuts.nextTab' },
  { id: 'prevTab', keys: 'Ctrl+Shift+Tab', label: 'shortcuts.prevTab' },
  { id: 'sidePanel', keys: 'Mod+.', label: 'shortcuts.sidePanel' },
  { id: 'search', keys: 'Mod+Shift+F', label: 'shortcuts.search' },
  { id: 'broadcast', keys: 'Mod+Shift+B', label: 'shortcuts.broadcast' },
  { id: 'splitMode', keys: 'Mod+Shift+M', label: 'shortcuts.splitMode' },
  { id: 'lock', keys: 'Mod+Shift+L', label: 'shortcuts.lock' },
  { id: 'settings', keys: 'Mod+,', label: 'shortcuts.settings' },
];

/** Matches a keyboard event against a shortcut string. */
export function matches(e: KeyboardEvent, keys: string, apple: boolean): boolean {
  const parts = keys.split('+');
  const key = parts[parts.length - 1]?.toLowerCase() ?? '';
  const want = {
    mod: parts.includes('Mod'),
    ctrl: parts.includes('Ctrl'),
    shift: parts.includes('Shift'),
    alt: parts.includes('Alt'),
  };
  const mod = apple ? e.metaKey : e.ctrlKey;
  const ctrl = apple ? e.ctrlKey : false;
  if (want.mod !== mod) return false;
  if (apple && want.ctrl !== ctrl) return false;
  if (!apple && want.ctrl && !e.ctrlKey) return false;
  if (want.shift !== e.shiftKey || want.alt !== e.altKey) return false;
  const k = e.key.toLowerCase();
  return k === key || (key === 'tab' && k === 'tab') || e.code.toLowerCase() === `key${key}`;
}
