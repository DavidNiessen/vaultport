// SPDX-License-Identifier: GPL-3.0-or-later
/**
 * Colours the command line while it is typed (Warp-style syntax highlighting), using xterm
 * decorations over the cells: the shell's own echo is never changed, so line editing,
 * history and the shell's completion keep working. Commands that do not exist on the machine
 * turn red once they are typed.
 *
 * The input is located from the shell integration mark (OSC 133;B) when there is one, else
 * backwards from the cursor. Without integration, colouring only happens right after the user
 * typed, so program output that merely looks like a prompt is left alone.
 */
import type { IDecoration, ITheme, Terminal } from '@xterm/xterm';

import type { Autocomplete } from './autocomplete/controller';
import { TOKEN_COLOR, tokenizeLine, type Known } from './syntax';

/** Decorations kept for command lines that already ran. */
const MAX_FROZEN = 3000;
/** Without shell integration, colour only this long after a key press. */
const TYPED_WINDOW_MS = 1500;

export interface InputHighlighter {
  /** Call after the user typed something. */
  noteTyping: () => void;
  /** Keeps the colours of the line that was just run (on OSC 133;C). */
  freeze: () => void;
  /** Removes the colours of the line being typed. */
  clear: () => void;
  dispose: () => void;
}

function isWide(ch: string): boolean {
  const c = ch.codePointAt(0) ?? 0;
  return (
    (c >= 0x1100 && c <= 0x115f) ||
    (c >= 0x2e80 && c <= 0xa4cf) ||
    (c >= 0xac00 && c <= 0xd7a3) ||
    (c >= 0xf900 && c <= 0xfaff) ||
    (c >= 0xfe30 && c <= 0xfe4f) ||
    (c >= 0xff00 && c <= 0xff60) ||
    (c >= 0xffe0 && c <= 0xffe6) ||
    (c >= 0x1f300 && c <= 0x1faff) ||
    (c >= 0x20000 && c <= 0x3fffd)
  );
}

/** Terminal columns taken by `s`. */
export function columns(s: string): number {
  let n = 0;
  for (const ch of s) n += isWide(ch) ? 2 : 1;
  return n;
}

/** `#rgb`/`#rrggbbaa` → `#rrggbb` (decorations only take that form). */
export function solidColor(c: string | undefined): string | null {
  if (!c) return null;
  const m = /^#([0-9a-f]{3}|[0-9a-f]{6}|[0-9a-f]{8})$/i.exec(c.trim());
  if (!m?.[1]) return null;
  const h = m[1];
  if (h.length === 3) return `#${h.replace(/./g, (x) => x + x)}`;
  return `#${h.slice(0, 6)}`;
}

/**
 * Cell segments `(row, col, width)` covering `text[start, end)`, for a line whose first
 * character is at absolute row `row0`, column `col0` in a terminal `cols` wide.
 */
export function segments(
  text: string,
  start: number,
  end: number,
  row0: number,
  col0: number,
  cols: number,
): { row: number; col: number; width: number }[] {
  const out: { row: number; col: number; width: number }[] = [];
  let offset = col0 + columns(text.slice(0, start));
  let remaining = columns(text.slice(start, end));
  while (remaining > 0 && cols > 0) {
    const row = row0 + Math.floor(offset / cols);
    const col = offset % cols;
    const width = Math.min(remaining, cols - col);
    out.push({ row, col, width });
    offset += width;
    remaining -= width;
  }
  return out;
}

export function attachInputHighlight(
  term: Terminal,
  deps: {
    enabled: () => boolean;
    ac: Autocomplete;
    known: Known;
  },
): InputHighlighter {
  let live: IDecoration[] = [];
  let frozen: IDecoration[] = [];
  let lastKey = '';
  let typedAt = 0;
  let frame = 0;

  const clear = () => {
    for (const d of live) d.dispose();
    live = [];
    lastKey = '';
  };

  const paint = () => {
    frame = 0;
    if (!deps.enabled()) {
      clear();
      return;
    }
    const phase = deps.ac.phase();
    if (phase === 'running' || phase === 'prompt') return;
    if (phase === 'unknown' && Date.now() - typedAt > TYPED_WINDOW_MS) return;
    const input = deps.ac.input();
    // No readable line (e.g. the cursor just left it on Enter): keep the colours as they are.
    if (!input) return;
    if (!input.line.trim()) {
      clear();
      return;
    }
    const buf = term.buffer.active;
    const cursorRow = buf.baseY + buf.cursorY;
    // Where the line starts: the integration mark, or back from the cursor.
    let row0: number;
    let col0: number;
    const mark = deps.ac.inputMark();
    if (mark) {
      row0 = mark.line;
      col0 = mark.col;
    } else {
      const back = columns(input.line.slice(0, input.cursor));
      const abs = cursorRow * term.cols + buf.cursorX - back;
      row0 = Math.floor(abs / term.cols);
      col0 = abs - row0 * term.cols;
    }
    const theme: ITheme = term.options.theme ?? {};
    const key = `${row0}:${col0}:${input.line}:${input.cursor}:${String(theme.green)}`;
    if (key === lastKey) return;
    clear();
    lastKey = key;
    for (const t of tokenizeLine(input.line, deps.known, input.cursor)) {
      const name = TOKEN_COLOR[t.kind];
      const color = name ? solidColor(theme[name as keyof ITheme] as string | undefined) : null;
      if (!color) continue;
      for (const seg of segments(input.line, t.start, t.end, row0, col0, term.cols)) {
        const marker = term.registerMarker(seg.row - cursorRow);
        const deco = term.registerDecoration({
          marker,
          x: seg.col,
          width: seg.width,
          foregroundColor: color,
          layer: 'top',
        });
        if (deco) live.push(deco);
        else marker.dispose();
      }
    }
  };

  const schedule = () => {
    if (!frame) frame = requestAnimationFrame(paint);
  };
  const sub = term.onWriteParsed(schedule);

  return {
    noteTyping: () => {
      typedAt = Date.now();
      schedule();
    },
    freeze: () => {
      if (frame) cancelAnimationFrame(frame);
      frame = 0;
      // The cursor already left the line, so keep what was painted while typing.
      frozen.push(...live);
      live = [];
      lastKey = '';
      if (frozen.length > MAX_FROZEN) {
        for (const d of frozen.splice(0, frozen.length - MAX_FROZEN)) d.dispose();
      }
    },
    clear,
    dispose: () => {
      if (frame) cancelAnimationFrame(frame);
      sub.dispose();
      clear();
      for (const d of frozen) d.dispose();
      frozen = [];
    },
  };
}
