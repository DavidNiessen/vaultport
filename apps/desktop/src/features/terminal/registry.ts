// SPDX-License-Identifier: GPL-3.0-or-later
/** Live terminal instances by tab id, used by snippets, broadcast input and workspaces. */
import type { Terminal } from '@xterm/xterm';

export interface TerminalHandle {
  term: Terminal;
  write: (data: string) => void;
  focus: () => void;
  sessionId: () => string | null;
  /** The shell's working directory, when it reports one (OSC 7). */
  cwd: () => string | null;
  /** Runs a terminal action (from the command palette or a menu). */
  action: (action: TerminalAction) => void;
}

/** Actions a terminal pane performs itself. */
export type TerminalAction =
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
  | 'installIntegration';

export const terminalRegistry = new Map<string, TerminalHandle>();

/** Sends input to one or many terminals (broadcast). */
export function sendInput(tabIds: string[], data: string): void {
  for (const id of tabIds) terminalRegistry.get(id)?.write(data);
}

/** Plain text of a terminal buffer (used by end-to-end tests in mock builds). */
export function terminalText(tabId?: string): string {
  const handles = tabId ? [terminalRegistry.get(tabId)] : [...terminalRegistry.values()];
  return handles
    .map((h) => {
      if (!h) return '';
      const buf = h.term.buffer.active;
      const lines: string[] = [];
      for (let i = 0; i < buf.length; i++)
        lines.push(buf.getLine(i)?.translateToString(true) ?? '');
      return lines.join('\n');
    })
    .join('\n');
}

if (import.meta.env.DEV || import.meta.env.VITE_MOCK_IPC === '1') {
  (window as unknown as { __terminalText?: typeof terminalText }).__terminalText = terminalText;
}
