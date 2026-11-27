// SPDX-License-Identifier: GPL-3.0-or-later
/**
 * Keyword highlighting: colors words like ERROR / WARN / INFO and IP addresses in newly written
 * lines using xterm decorations (the output stream itself is never modified).
 */
import type { IDecoration, Terminal } from '@xterm/xterm';

interface Rule {
  re: RegExp;
  color: string;
}

const RULES: Rule[] = [
  { re: /\b(error|err|fatal|failed|failure|critical|panic|denied|refused)\b/gi, color: '#f2687a' },
  { re: /\b(warn|warning|deprecated|timeout)\b/gi, color: '#f4c15c' },
  { re: /\b(info|notice|ok|success|succeeded|done|active \(running\))\b/gi, color: '#5fd69b' },
  { re: /\b(?:\d{1,3}\.){3}\d{1,3}(?::\d+)?\b/g, color: '#6f9cff' },
];

const scanned = new WeakMap<Terminal, number>();
const live = new WeakMap<Terminal, IDecoration[]>();
const MAX_DECORATIONS = 2000;

/** Decorates lines written since the last call. Cheap enough to call after every write. */
export function highlightKeywords(term: Terminal): void {
  const buf = term.buffer.active;
  const end = buf.baseY + buf.cursorY;
  const start = Math.max(scanned.get(term) ?? 0, end - term.rows);
  scanned.set(term, end);
  const decorations = live.get(term) ?? [];
  for (let y = start; y < end; y++) {
    const line = buf.getLine(y);
    if (!line || line.isWrapped) continue;
    const text = line.translateToString(true);
    if (text.length === 0 || text.length > 2000) continue;
    for (const rule of RULES) {
      rule.re.lastIndex = 0;
      let m: RegExpExecArray | null;
      while ((m = rule.re.exec(text)) !== null) {
        const marker = term.registerMarker(y - (buf.baseY + buf.cursorY));
        const deco = term.registerDecoration({
          marker,
          x: m.index,
          width: m[0].length,
          foregroundColor: rule.color,
        });
        if (deco) decorations.push(deco);
      }
    }
  }
  while (decorations.length > MAX_DECORATIONS) decorations.shift()?.dispose();
  live.set(term, decorations);
}
