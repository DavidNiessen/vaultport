// SPDX-License-Identifier: GPL-3.0-or-later
/**
 * Output highlighting: colours words like ERROR / WARN / INFO, IP and MAC addresses, dates,
 * UUIDs, hashes and HTTP status codes in newly written lines, plus the user's own rules, using
 * xterm decorations (the output stream itself is never modified).
 */
import type { IDecoration, Terminal } from '@xterm/xterm';

import type { HighlightRule } from './settings';

export interface Rule {
  re: RegExp;
  color: string;
}

export const DEFAULT_RULES: Rule[] = [
  {
    re: /\b(error|err|fatal|failed|failure|critical|panic|denied|refused|exception)\b/gi,
    color: '#f2687a',
  },
  { re: /\b(warn|warning|deprecated|timeout|timed out)\b/gi, color: '#f4c15c' },
  {
    re: /\b(info|notice|ok|success|succeeded|done|passed|active \(running\))\b/gi,
    color: '#5fd69b',
  },
  { re: /\b(debug|trace|verbose)\b/gi, color: '#8a8aa3' },
  // HTTP status codes after a protocol or "status".
  { re: /(?<=\bHTTP\/[\d.]+\s|\bstatus[=: ]\s?)[45]\d\d\b/gi, color: '#f2687a' },
  { re: /(?<=\bHTTP\/[\d.]+\s|\bstatus[=: ]\s?)[23]\d\d\b/gi, color: '#5fd69b' },
  // ISO dates and times.
  {
    re: /\b\d{4}-\d{2}-\d{2}(?:[T ]\d{2}:\d{2}(?::\d{2}(?:[.,]\d+)?)?(?:Z|[+-]\d{2}:?\d{2})?)?\b/g,
    color: '#b9a3ff',
  },
  { re: /\b[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\b/gi, color: '#5fd6e3' },
  { re: /\b(?:[0-9a-f]{2}:){5}[0-9a-f]{2}\b/gi, color: '#5fd6e3' },
  { re: /\b(?:\d{1,3}\.){3}\d{1,3}(?:\/\d{1,2})?(?::\d+)?\b/g, color: '#6f9cff' },
  { re: /\b(?:[0-9a-f]{1,4}:){3,7}[0-9a-f]{1,4}\b/gi, color: '#6f9cff' },
  // Commit hashes and digests (with at least one letter and one digit).
  { re: /\b(?=[0-9a-f]*[a-f])(?=[0-9a-f]*\d)[0-9a-f]{7,64}\b/g, color: '#f0a8e6' },
];

/** Compiles the user's rules, skipping invalid ones. */
export function compileRules(rules: HighlightRule[]): Rule[] {
  const out: Rule[] = [];
  for (const r of rules) {
    if (!r.pattern || !/^#[0-9a-f]{6}$/i.test(r.color)) continue;
    try {
      const re = new RegExp(r.pattern, r.caseSensitive ? 'g' : 'gi');
      // A pattern that matches the empty string would never advance.
      if (re.test('')) continue;
      out.push({ re, color: r.color });
    } catch {
      // Invalid: shown as such in Settings.
    }
  }
  return out;
}

/** Whether `pattern` is a usable regular expression. */
export function isValidPattern(pattern: string): boolean {
  try {
    return pattern !== '' && !new RegExp(pattern).test('');
  } catch {
    return false;
  }
}

const scanned = new WeakMap<Terminal, number>();
const live = new WeakMap<Terminal, IDecoration[]>();
const MAX_DECORATIONS = 2000;
/** Matches decorated per line at most (very busy lines are left alone). */
const MAX_PER_LINE = 40;

/** Decorates lines written since the last call. Cheap enough to call after every write. */
export function highlightKeywords(term: Terminal, extra: Rule[] = []): void {
  const buf = term.buffer.active;
  const end = buf.baseY + buf.cursorY;
  const start = Math.max(scanned.get(term) ?? 0, end - term.rows);
  scanned.set(term, end);
  const decorations = live.get(term) ?? [];
  const rules = extra.length ? [...DEFAULT_RULES, ...extra] : DEFAULT_RULES;
  for (let y = start; y < end; y++) {
    const line = buf.getLine(y);
    if (!line || line.isWrapped) continue;
    const text = line.translateToString(true);
    if (text.length === 0 || text.length > 2000) continue;
    // Later rules (the user's) win where matches overlap.
    const taken: [number, number][] = [];
    let count = 0;
    for (const rule of [...rules].reverse()) {
      rule.re.lastIndex = 0;
      let m: RegExpExecArray | null;
      while ((m = rule.re.exec(text)) !== null && count < MAX_PER_LINE) {
        if (m[0].length === 0) {
          rule.re.lastIndex++;
          continue;
        }
        const from = m.index;
        const to = from + m[0].length;
        if (taken.some(([a, b]) => from < b && to > a)) continue;
        taken.push([from, to]);
        count++;
        const marker = term.registerMarker(y - (buf.baseY + buf.cursorY));
        const deco = term.registerDecoration({
          marker,
          x: from,
          width: m[0].length,
          foregroundColor: rule.color,
        });
        if (deco) decorations.push(deco);
        else marker.dispose();
      }
    }
  }
  while (decorations.length > MAX_DECORATIONS) {
    const d = decorations.shift();
    d?.dispose();
    d?.marker.dispose();
  }
  live.set(term, decorations);
}
