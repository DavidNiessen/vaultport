// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from 'vitest';

import type { CompletionItem } from '@/lib/ipc';

import { acceptKeys } from './controller';
import { readInput, wordBefore, type BufferLike } from './inputLine';

/** A screen of `cols` columns; rows starting with `↪` continue the previous row. */
function screen(rows: string[], cursor: [number, number], type: BufferLike['type'] = 'normal') {
  const lines = rows.map((r) => {
    const isWrapped = r.startsWith('↪');
    const text = isWrapped ? r.slice(1) : r;
    return {
      isWrapped,
      translateToString: (trimRight = false, start = 0, end?: number) => {
        const s = text.slice(start, end);
        return trimRight ? s.trimEnd() : s;
      },
    };
  });
  const buf: BufferLike = {
    type,
    cursorX: cursor[0],
    cursorY: cursor[1],
    baseY: 0,
    getLine: (y) => lines[y],
  };
  return buf;
}

describe('readInput', () => {
  it('reads from the shell integration mark to the end of the line', () => {
    const buf = screen(['user@host:~$ git checkout main'], [20, 0]);
    expect(readInput(buf, { line: 0, col: 13 })).toEqual({
      line: 'git checkout main',
      cursor: 7,
    });
  });

  it('keeps a trailing space before the cursor', () => {
    const buf = screen(['$ git '], [6, 0]);
    expect(readInput(buf, { line: 0, col: 2 })).toEqual({ line: 'git ', cursor: 4 });
  });

  it('joins wrapped rows', () => {
    const buf = screen(['$ echo aaaa', '↪bbbb cc'], [7, 1]);
    expect(readInput(buf, { line: 0, col: 2 })).toEqual({ line: 'echo aaaabbbb cc', cursor: 16 });
  });

  it('gives up on multi-line input after the mark', () => {
    const buf = screen(['$ for i in 1 2; do', '> echo $i'], [9, 1]);
    expect(readInput(buf, { line: 0, col: 2 })).toBeNull();
  });

  it('finds the prompt without shell integration', () => {
    expect(readInput(screen(['root@box:/etc# ls -l'], [20, 0]), null)).toEqual({
      line: 'ls -l',
      cursor: 5,
    });
    expect(readInput(screen(['~/src ❯ git st'], [14, 0]), null)?.line).toBe('git st');
  });

  it('takes the first prompt terminator, not one in the command', () => {
    const buf = screen(['$ echo a > b'], [12, 0]);
    expect(readInput(buf, null)).toEqual({ line: 'echo a > b', cursor: 10 });
  });

  it('ignores full-screen programs and lines without a prompt', () => {
    expect(readInput(screen(['$ vim'], [5, 0], 'alternate'), null)).toBeNull();
    expect(readInput(screen(['Downloading 45%'], [15, 0]), null)).toBeNull();
  });
});

describe('wordBefore', () => {
  it('returns the word at the cursor', () => {
    expect(wordBefore({ line: 'git chec main', cursor: 8 })).toBe('chec');
    expect(wordBefore({ line: 'git ', cursor: 4 })).toBe('');
  });
});

describe('acceptKeys', () => {
  const item = (over: Partial<CompletionItem>): CompletionItem => ({
    text: 'checkout',
    label: 'checkout',
    description: null,
    kind: 'subcommand',
    deleteBefore: 2,
    deleteAfter: 0,
    ...over,
  });

  it('deletes the typed part, types the suggestion and a space', () => {
    expect(acceptKeys(item({}))).toBe('\x7f\x7fcheckout ');
  });

  it('leaves directories and options with a value open', () => {
    expect(acceptKeys(item({ kind: 'path', text: 'src/', deleteBefore: 1 }))).toBe('\x7fsrc/');
    expect(acceptKeys(item({ kind: 'option', text: '--color=', deleteBefore: 0 }))).toBe(
      '--color=',
    );
  });

  it('does not add a space after a whole command from history', () => {
    expect(
      acceptKeys(item({ kind: 'history', text: 'git status', deleteBefore: 3, deleteAfter: 1 })),
    ).toBe('\x7f\x7f\x7f\x1b[3~git status');
  });
});
