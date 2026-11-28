// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from 'vitest';

import type { CompletionItem } from '@/lib/ipc';

import { acceptKeys, ghostFor, ghostWord } from './controller';
import { highlightRuns, matchIndexes, queryFor } from './match';

const item = (p: Partial<CompletionItem>): CompletionItem => ({
  text: '',
  label: '',
  description: null,
  kind: 'history',
  deleteBefore: 0,
  deleteAfter: 0,
  ...p,
});

describe('match highlighting', () => {
  it('prefers a prefix, else a subsequence', () => {
    expect(matchIndexes('checkout', 'che')).toEqual([0, 1, 2]);
    expect(matchIndexes('checkout', 'chkt')).toEqual([0, 1, 4, 7]);
    expect(matchIndexes('checkout', 'x')).toEqual([]);
    expect(highlightRuns('abc', [0, 2])).toEqual([
      { text: 'a', hit: true },
      { text: 'b', hit: false },
      { text: 'c', hit: true },
    ]);
  });

  it('matches paths by their last segment', () => {
    expect(queryFor('src/comp', 'path')).toBe('comp');
    expect(queryFor('"$HO', 'value')).toBe('$HO');
    expect(queryFor('--format=js', 'value')).toBe('js');
  });
});

describe('ghost text', () => {
  it('continues the line with the best history match', () => {
    const items = [
      item({ kind: 'command', text: 'gitk', deleteBefore: 3 }),
      item({ text: 'git push origin main', deleteBefore: 6 }),
    ];
    expect(ghostFor({ line: 'git pu', cursor: 6 }, items)).toBe('sh origin main');
    // Not in the middle of the line, nor for history that does not continue it.
    expect(ghostFor({ line: 'git pu', cursor: 3 }, items)).toBeNull();
    expect(ghostFor({ line: 'git pull', cursor: 8 }, items)).toBeNull();
    // A history match for the command after a pipe replaces only that command.
    expect(
      ghostFor({ line: 'ls | gr', cursor: 7 }, [item({ text: 'grep -i x', deleteBefore: 2 })]),
    ).toBe('ep -i x');
    expect(ghostWord(' origin main')).toBe(' origin');
  });

  it('types the keys that insert a suggestion', () => {
    expect(acceptKeys(item({ kind: 'subcommand', text: 'checkout', deleteBefore: 2 }))).toBe(
      '\x7f\x7fcheckout ',
    );
    expect(acceptKeys(item({ kind: 'path', text: 'src/', deleteBefore: 1 }))).toBe('\x7fsrc/');
  });
});
