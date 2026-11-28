// SPDX-License-Identifier: GPL-3.0-or-later
/** Ranking for the command history search. */
import { matchIndexes } from './autocomplete/match';

export interface HistoryEntry {
  command: string;
  /** Runs counted by the history index (session commands count once). */
  count: number;
  /** From this terminal session. */
  session: boolean;
}

/**
 * Ranks `entries` for `query`: prefix matches first, then substring, then fuzzy matches;
 * within a group, recent session commands and frequent commands first.
 */
export function rankHistory(entries: HistoryEntry[], query: string): HistoryEntry[] {
  const q = query.trim().toLowerCase();
  const scored: { e: HistoryEntry; score: number; i: number }[] = [];
  entries.forEach((e, i) => {
    const c = e.command.toLowerCase();
    let score: number;
    if (!q) score = 0;
    else if (c.startsWith(q)) score = 3000;
    else if (c.includes(q)) score = 2000 - Math.min(500, c.indexOf(q));
    else if (q.split(/\s+/).every((part) => c.includes(part))) score = 1500;
    else if (matchIndexes(c, q).length) score = 1000;
    else return;
    score += (e.session ? 200 : 0) + Math.min(150, Math.log2(1 + e.count) * 30);
    scored.push({ e, score, i });
  });
  scored.sort((a, b) => b.score - a.score || a.i - b.i);
  return scored.map((s) => s.e);
}

/** Session commands (newest first) followed by stored history, without duplicates. */
export function mergeHistory(
  session: string[],
  stored: { command: string; count: number }[],
): HistoryEntry[] {
  const seen = new Set<string>();
  const out: HistoryEntry[] = [];
  for (const c of [...session].reverse()) {
    const cmd = c.trim();
    if (!cmd || seen.has(cmd)) continue;
    seen.add(cmd);
    out.push({ command: cmd, count: 1, session: true });
  }
  for (const h of stored) {
    if (seen.has(h.command)) continue;
    seen.add(h.command);
    out.push({ command: h.command, count: h.count, session: false });
  }
  return out;
}
