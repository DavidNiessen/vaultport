// SPDX-License-Identifier: GPL-3.0-or-later
/** Which characters of a suggestion match what was typed, for highlighting. */

/** The part of a typed word that a label is matched against (paths: the last segment). */
export function queryFor(word: string, kind: string): string {
  let w = word.replace(/^["']/, '');
  if (kind === 'path') w = w.slice(w.lastIndexOf('/') + 1);
  if (kind === 'value' && w.includes('=') && w.startsWith('-')) w = w.slice(w.indexOf('=') + 1);
  const dollar = w.lastIndexOf('$');
  if (dollar >= 0) w = w.slice(dollar);
  return w;
}

/**
 * Indexes of `label` that match `query`: a case-insensitive prefix, else the earliest
 * subsequence. Empty when nothing matches.
 */
export function matchIndexes(label: string, query: string): number[] {
  if (!query) return [];
  const l = label.toLowerCase();
  const q = query.toLowerCase();
  if (l.startsWith(q)) return [...Array(q.length).keys()];
  const out: number[] = [];
  let from = 0;
  for (const c of q) {
    const i = l.indexOf(c, from);
    if (i < 0) return [];
    out.push(i);
    from = i + 1;
  }
  return out;
}

/** Splits `label` into runs of matched and unmatched text. */
export function highlightRuns(label: string, indexes: number[]): { text: string; hit: boolean }[] {
  const hits = new Set(indexes);
  const runs: { text: string; hit: boolean }[] = [];
  for (let i = 0; i < label.length; i++) {
    const hit = hits.has(i);
    const last = runs[runs.length - 1];
    if (last?.hit === hit) last.text += label.charAt(i);
    else runs.push({ text: label.charAt(i), hit });
  }
  return runs;
}
