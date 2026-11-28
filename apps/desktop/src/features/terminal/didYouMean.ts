// SPDX-License-Identifier: GPL-3.0-or-later
/**
 * "Did you mean …?" for a command that was not found: the closest installed command by
 * Damerau–Levenshtein distance (a swap of two neighbours counts as one edit, so `gti` → `git`).
 */

/** Edit distance with adjacent transpositions, giving up above `max`. */
export function editDistance(a: string, b: string, max = 3): number {
  if (Math.abs(a.length - b.length) > max) return max + 1;
  const cols = b.length + 1;
  // Row-major (a.length + 1) × (b.length + 1) table.
  const d = new Int32Array((a.length + 1) * cols);
  const at = (i: number, j: number) => d[i * cols + j] ?? 0;
  for (let j = 0; j < cols; j++) d[j] = j;
  for (let i = 1; i <= a.length; i++) {
    d[i * cols] = i;
    let best = i;
    for (let j = 1; j < cols; j++) {
      const cost = a[i - 1] === b[j - 1] ? 0 : 1;
      let v = Math.min(at(i - 1, j) + 1, at(i, j - 1) + 1, at(i - 1, j - 1) + cost);
      if (i > 1 && j > 1 && a[i - 1] === b[j - 2] && a[i - 2] === b[j - 1]) {
        v = Math.min(v, at(i - 2, j - 2) + 1);
      }
      d[i * cols + j] = v;
      best = Math.min(best, v);
    }
    if (best > max) return max + 1;
  }
  return at(a.length, b.length);
}

/**
 * The installed command closest to `typo`, or `null` if none is close enough (one edit for
 * short names, two for names of five letters or more).
 */
export function closestCommand(typo: string, known: Iterable<string>): string | null {
  if (typo.length < 2 || /[/\s]/.test(typo)) return null;
  const max = typo.length >= 5 ? 2 : 1;
  let best: string | null = null;
  let bestScore = Infinity;
  for (const k of known) {
    if (k === typo || Math.abs(k.length - typo.length) > max) continue;
    const dist = editDistance(typo, k, max);
    if (dist > max) continue;
    // Same first letter and same length break ties.
    const score =
      dist * 10 + (k.startsWith(typo.charAt(0)) ? 0 : 3) + Math.abs(k.length - typo.length);
    if (score < bestScore || (score === bestScore && best !== null && k < best)) {
      best = k;
      bestScore = score;
    }
  }
  return best;
}

/** The command line with its first word replaced by `fixed`. */
export function fixCommandLine(line: string, fixed: string): string {
  return line.replace(/^(\s*(?:\w+=\S*\s+)*)\S+/, (_, prefix: string) => `${prefix}${fixed}`);
}

/** Whether a finished command failed because the command does not exist. */
export function isNotFound(exit: number | null, output: string): boolean {
  return exit === 127 || /command not found|not recognized as|Unknown command/i.test(output);
}
