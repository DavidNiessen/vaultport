// SPDX-License-Identifier: GPL-3.0-or-later
/** Live terminal instances by tab id, used by snippets, broadcast input and workspaces. */
import type { Terminal } from '@xterm/xterm';

export interface TerminalHandle {
  term: Terminal;
  write: (data: string) => void;
  focus: () => void;
  sessionId: () => string | null;
}

export const terminalRegistry = new Map<string, TerminalHandle>();

/** Sends input to one or many terminals (broadcast). */
export function sendInput(tabIds: string[], data: string): void {
  for (const id of tabIds) terminalRegistry.get(id)?.write(data);
}
