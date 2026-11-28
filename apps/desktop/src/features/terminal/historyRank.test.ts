// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from 'vitest';

import { badgeText, blockText, formatDuration, unescapeCommand } from './blocks';
import { mergeHistory, rankHistory } from './historyRank';

describe('history search', () => {
  const all = mergeHistory(
    ['git status', 'make deploy', 'git status'],
    [
      { command: 'git push origin main', count: 30 },
      { command: 'make deploy', count: 4 },
      { command: 'docker compose up -d', count: 2 },
    ],
  );

  it('merges session commands first, without duplicates', () => {
    expect(all.map((e) => e.command)).toEqual([
      'git status',
      'make deploy',
      'git push origin main',
      'docker compose up -d',
    ]);
    expect(all[0]?.session).toBe(true);
  });

  it('ranks prefix, substring, words and fuzzy matches', () => {
    expect(rankHistory(all, 'git').map((e) => e.command)).toEqual([
      'git status',
      'git push origin main',
    ]);
    expect(rankHistory(all, 'up -d')[0]?.command).toBe('docker compose up -d');
    expect(rankHistory(all, 'origin push')[0]?.command).toBe('git push origin main');
    expect(rankHistory(all, 'dcu')[0]?.command).toBe('docker compose up -d');
    expect(rankHistory(all, 'zzz')).toEqual([]);
    expect(rankHistory(all, '')).toHaveLength(4);
  });
});

describe('blocks', () => {
  it('formats durations and badges', () => {
    expect(formatDuration(420)).toBe('420ms');
    expect(formatDuration(1234)).toBe('1.2s');
    expect(formatDuration(42_000)).toBe('42s');
    expect(formatDuration(184_000)).toBe('3m 04s');
    expect(formatDuration(3_720_000)).toBe('1h 02m');
    expect(badgeText(0, 1234)).toBe('✓ · 1.2s');
    expect(badgeText(127, null)).toBe('✗ 127');
  });

  it('decodes command lines reported by the shell', () => {
    expect(unescapeCommand('echo a\\x3bb\\\\c')).toBe('echo a;b\\c');
    expect(blockText('ls', 'a\nb')).toBe('$ ls\na\nb');
  });
});
