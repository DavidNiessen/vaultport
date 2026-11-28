// SPDX-License-Identifier: GPL-3.0-or-later
/**
 * Finds the command line the user is typing at the prompt, from what the terminal shows.
 *
 * With shell integration, OSC 133;B marks where input starts. Without it, the prompt is
 * recognised by its usual terminator (`$ `, `# `, `% `, `> `, `❯ `). Reading the screen
 * rather than recording keystrokes keeps line editing, history recall and the shell's own
 * completion in step.
 */

/** The parts of xterm's `IBuffer` this needs. */
export interface BufferLike {
  readonly type: 'normal' | 'alternate';
  readonly cursorX: number;
  readonly cursorY: number;
  readonly baseY: number;
  getLine(y: number): LineLike | undefined;
}

export interface LineLike {
  readonly isWrapped: boolean;
  translateToString(trimRight?: boolean, startColumn?: number, endColumn?: number): string;
}

/** Where input started (absolute buffer row and column), from OSC 133;B. */
export interface InputMark {
  line: number;
  col: number;
}

export interface InputLine {
  line: string;
  /** Cursor position in `line`, in UTF-16 code units. */
  cursor: number;
}

/** Rows a single command line may span before it is not worth completing. */
const MAX_ROWS = 12;
const PROMPT_ENDS = ['$ ', '# ', '% ', '> ', '❯ ', '» '];
/** A prompt is expected near the start of its row. */
const PROMPT_WINDOW = 120;

/** The input line at the cursor, or `null` when there is nothing to complete. */
export function readInput(buf: BufferLike, mark: InputMark | null): InputLine | null {
  if (buf.type === 'alternate') return null;
  const row = buf.baseY + buf.cursorY;
  const current = buf.getLine(row);
  if (!current) return null;

  let start = row;
  if (mark) {
    if (mark.line > row || row - mark.line > MAX_ROWS) return null;
    // Every row after the mark must continue it; a new row means multi-line input (PS2).
    for (let y = row; y > mark.line; y--) {
      if (!buf.getLine(y)?.isWrapped) return null;
    }
    start = mark.line;
  } else {
    while (start > 0 && row - start < MAX_ROWS && buf.getLine(start)?.isWrapped) start--;
  }

  let before = '';
  for (let y = start; y < row; y++) {
    const text = buf.getLine(y)?.translateToString(false) ?? '';
    before += y === start && mark ? text.slice(columnIndex(text, mark.col)) : text;
  }
  const head = current.translateToString(false, 0, buf.cursorX);
  before += start === row && mark ? head.slice(columnIndex(head, mark.col)) : head;
  let after = current.translateToString(true, buf.cursorX);
  for (let y = row + 1; y - row < MAX_ROWS && buf.getLine(y)?.isWrapped; y++) {
    after += buf.getLine(y)?.translateToString(true) ?? '';
  }

  if (!mark) {
    const window = before.slice(0, PROMPT_WINDOW);
    let end = -1;
    for (const p of PROMPT_ENDS) {
      const i = window.indexOf(p);
      if (i >= 0 && (end < 0 || i + p.length < end)) end = i + p.length;
    }
    if (end < 0) return null;
    before = before.slice(end);
  }
  // Cells a shell cleared by writing spaces are not trimmed by xterm; they are not input.
  after = after.replace(/\s+$/, '');
  return { line: before + after, cursor: before.length };
}

/**
 * String index of terminal column `col` in a row's text. Wide characters fill two columns but
 * are one string position, so count them as xterm lays them out.
 */
function columnIndex(text: string, col: number): number {
  let column = 0;
  let i = 0;
  for (const ch of text) {
    if (column >= col) break;
    column += isWide(ch) ? 2 : 1;
    i += ch.length;
  }
  return i;
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

/** The word before the cursor (as typed, quotes included), for hiding exact matches. */
export function wordBefore(input: InputLine): string {
  const head = input.line.slice(0, input.cursor);
  const m = /\S*$/.exec(head);
  return m ? m[0] : '';
}
