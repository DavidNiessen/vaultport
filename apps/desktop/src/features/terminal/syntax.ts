// SPDX-License-Identifier: GPL-3.0-or-later
/**
 * A small, forgiving shell lexer for colouring the command line while it is typed: commands
 * (known, unknown, still being typed), options, strings, variables, operators, keywords,
 * assignments, paths and comments. It never fails; odd input just gets fewer colours.
 */

export type TokenKind =
  | 'command'
  | 'unknownCommand'
  | 'keyword'
  | 'option'
  | 'string'
  | 'variable'
  | 'operator'
  | 'comment'
  | 'assignment'
  | 'path'
  | 'argument';

export interface Token {
  /** UTF-16 offsets in the line. */
  start: number;
  end: number;
  kind: TokenKind;
}

/** Commands whose first non-option argument is another command. */
const WRAPPERS = new Set([
  'sudo',
  'doas',
  'env',
  'time',
  'nohup',
  'nice',
  'exec',
  'command',
  'builtin',
  'xargs',
  'watch',
  'caffeinate',
  'chronic',
  'stdbuf',
  'unbuffer',
  'npx',
  'bunx',
]);

const KEYWORDS = new Set([
  'if',
  'then',
  'else',
  'elif',
  'fi',
  'for',
  'while',
  'until',
  'do',
  'done',
  'case',
  'esac',
  'in',
  'function',
  'select',
  'time',
  '!',
  '{',
  '}',
  '[[',
  ']]',
]);

/** Keywords after which a command follows. */
const COMMAND_AFTER = new Set(['then', 'else', 'elif', 'do', 'if', 'while', 'until', '!', '{']);

const OPERATORS = [
  '2>&1',
  '1>&2',
  '>&2',
  '>&1',
  '&>>',
  '&&',
  '||',
  ';;',
  '|&',
  '>>',
  '<<',
  '&>',
  '2>',
  '>&',
  '<&',
  '|',
  ';',
  '&',
  '>',
  '<',
  '(',
  ')',
];
/** Operators that start a new command. */
const SEPARATORS = new Set(['&&', '||', ';;', '|&', '|', ';', '&', '(']);

export type Known = (command: string) => boolean | null;

interface Word {
  start: number;
  end: number;
  text: string;
}

/** Splits a word into string / variable runs; the rest gets `base`. */
function wordTokens(line: string, w: Word, base: TokenKind): Token[] {
  const out: Token[] = [];
  let i = w.start;
  let runStart = i;
  const flush = (to: number, kind: TokenKind) => {
    if (to > runStart) out.push({ start: runStart, end: to, kind });
    runStart = to;
  };
  while (i < w.end) {
    const c = line[i];
    if (c === "'") {
      flush(i, base);
      const close = line.indexOf("'", i + 1);
      i = close < 0 || close >= w.end ? w.end : close + 1;
      flush(i, 'string');
    } else if (c === '"') {
      flush(i, base);
      let j = i + 1;
      while (j < w.end && line[j] !== '"') j += line[j] === '\\' ? 2 : 1;
      i = Math.min(w.end, j + 1);
      // Variables inside double quotes stay visible.
      const inner = line.slice(runStart, i);
      const re = /\$(\{[^}]*\}?|[A-Za-z_][A-Za-z0-9_]*|[?$!#@*0-9])/g;
      let last = runStart;
      for (const m of inner.matchAll(re)) {
        const s = runStart + m.index;
        if (s > last) out.push({ start: last, end: s, kind: 'string' });
        out.push({ start: s, end: s + m[0].length, kind: 'variable' });
        last = s + m[0].length;
      }
      if (i > last) out.push({ start: last, end: i, kind: 'string' });
      runStart = i;
    } else if (c === '$') {
      flush(i, base);
      const m = /^\$(\{[^}]*\}?|\(\(?|[A-Za-z_][A-Za-z0-9_]*|[?$!#@*0-9])?/.exec(
        line.slice(i, w.end),
      );
      i += Math.max(1, m?.[0].length ?? 1);
      flush(i, 'variable');
    } else if (c === '\\') {
      i += 2;
    } else {
      i++;
    }
  }
  flush(Math.min(i, w.end), base);
  return out;
}

/**
 * Tokens of `line`. `known` says whether a command exists (`null`: cannot tell). A command
 * word that ends at `cursor` is still being typed and is never marked unknown.
 */
export function tokenizeLine(line: string, known: Known, cursor = -1): Token[] {
  const tokens: Token[] = [];
  let i = 0;
  let expectCommand = true;
  /** After a wrapper: skip its options before the wrapped command. */
  let afterWrapper = false;
  const n = line.length;
  while (i < n) {
    const c = line[i] ?? '';
    if (c === ' ' || c === '\t') {
      i++;
      continue;
    }
    if (c === '#' && (i === 0 || /\s/.test(line[i - 1] ?? ''))) {
      tokens.push({ start: i, end: n, kind: 'comment' });
      break;
    }
    const op = OPERATORS.find((o) => line.startsWith(o, i));
    if (op) {
      tokens.push({ start: i, end: i + op.length, kind: 'operator' });
      i += op.length;
      if (SEPARATORS.has(op)) {
        expectCommand = true;
        afterWrapper = false;
      }
      continue;
    }
    // A word: up to unquoted whitespace or operator.
    const start = i;
    while (i < n) {
      const ch = line[i] ?? '';
      if (ch === ' ' || ch === '\t') break;
      if (ch === '\\') {
        i += 2;
        continue;
      }
      if (ch === "'" || ch === '"') {
        const close = line.indexOf(ch, i + 1);
        i = close < 0 ? n : close + 1;
        continue;
      }
      if (ch === '$' && line[i + 1] === '(') {
        // Command substitution: colour its inside as a nested line.
        let depth = 0;
        let j = i + 1;
        for (; j < n; j++) {
          if (line[j] === '(') depth++;
          else if (line[j] === ')' && --depth === 0) break;
        }
        i = Math.min(n, j + 1);
        continue;
      }
      if (OPERATORS.some((o) => line.startsWith(o, i))) break;
      i++;
    }
    const w: Word = {
      start,
      end: Math.max(i, start + 1),
      text: line.slice(start, Math.max(i, start + 1)),
    };
    i = w.end;

    if (w.text.startsWith('$(')) {
      // Command substitution: its inside is a command line of its own.
      tokens.push({ start: w.start, end: w.start + 2, kind: 'operator' });
      const closed = w.text.endsWith(')') && w.text.length > 2;
      const inner = line.slice(w.start + 2, closed ? w.end - 1 : w.end);
      for (const t of tokenizeLine(inner, known, cursor - w.start - 2)) {
        tokens.push({ ...t, start: t.start + w.start + 2, end: t.end + w.start + 2 });
      }
      if (closed) tokens.push({ start: w.end - 1, end: w.end, kind: 'operator' });
      expectCommand = false;
      continue;
    }

    if (expectCommand) {
      if (/^[A-Za-z_][A-Za-z0-9_]*=/.test(w.text)) {
        const eq = w.text.indexOf('=');
        tokens.push({ start: w.start, end: w.start + eq + 1, kind: 'assignment' });
        tokens.push(...wordTokens(line, { ...w, start: w.start + eq + 1 }, 'argument'));
        continue;
      }
      if (afterWrapper && w.text.startsWith('-')) {
        tokens.push(...wordTokens(line, w, 'option'));
        continue;
      }
      if (KEYWORDS.has(w.text) && !afterWrapper) {
        tokens.push({ start: w.start, end: w.end, kind: 'keyword' });
        expectCommand = COMMAND_AFTER.has(w.text);
        continue;
      }
      const name = w.text.replace(/^\\/, '');
      let kind: TokenKind = 'command';
      if (/["'$`]/.test(name)) kind = 'command';
      else if (!name.includes('/')) {
        const k = known(name);
        if (k === false && w.end !== cursor) kind = 'unknownCommand';
      }
      tokens.push(...wordTokens(line, w, kind));
      expectCommand = false;
      afterWrapper = WRAPPERS.has(name);
      if (afterWrapper) expectCommand = true;
      continue;
    }

    let base: TokenKind = 'argument';
    if (w.text.startsWith('-') && w.text.length > 1) base = 'option';
    else if (/^(~|\.{1,2})?\//.test(w.text) || /^\.{1,2}$/.test(w.text) || w.text.startsWith('~'))
      base = 'path';
    if (base === 'option' && w.text.includes('=')) {
      const eq = w.text.indexOf('=');
      tokens.push({ start: w.start, end: w.start + eq + 1, kind: 'option' });
      tokens.push(...wordTokens(line, { ...w, start: w.start + eq + 1 }, 'argument'));
    } else {
      tokens.push(...wordTokens(line, w, base));
    }
  }
  return tokens.filter((t) => t.end > t.start);
}

/** Theme colour (an ANSI palette name) used for each token kind. `null`: leave as is. */
export const TOKEN_COLOR: Record<TokenKind, string | null> = {
  command: 'green',
  unknownCommand: 'red',
  keyword: 'magenta',
  option: 'cyan',
  string: 'yellow',
  variable: 'magenta',
  operator: 'blue',
  comment: 'brightBlack',
  assignment: 'magenta',
  path: 'blue',
  argument: null,
};
