// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from 'vitest';

import { DEFAULT_RULES, compileRules, isValidPattern } from './highlight';

function hits(text: string): string[] {
  const out: string[] = [];
  for (const r of DEFAULT_RULES) {
    r.re.lastIndex = 0;
    for (const m of text.matchAll(r.re)) out.push(m[0]);
  }
  return out;
}

describe('output highlighting', () => {
  it('finds the usual suspects', () => {
    expect(hits('2026-11-27T10:00:01Z ERROR request 7f3a2c1 failed')).toEqual(
      expect.arrayContaining(['2026-11-27T10:00:01Z', 'ERROR', 'failed', '7f3a2c1']),
    );
    expect(hits('HTTP/1.1 503 Service Unavailable')).toContain('503');
    expect(hits('ether 02:42:ac:11:00:02 inet 172.17.0.2/16')).toEqual(
      expect.arrayContaining(['02:42:ac:11:00:02', '172.17.0.2/16']),
    );
    expect(hits('id 123e4567-e89b-12d3-a456-426614174000')).toContain(
      '123e4567-e89b-12d3-a456-426614174000',
    );
    // Plain numbers and words are not hashes.
    expect(hits('copied 12345678 bytes in deadbeef')).not.toContain('12345678');
  });

  it('compiles user rules and skips invalid ones', () => {
    const rules = compileRules([
      { pattern: 'prod-\\w+', color: '#ff0000', caseSensitive: false },
      { pattern: '(', color: '#ff0000', caseSensitive: false },
      { pattern: 'x*', color: '#ff0000', caseSensitive: false },
      { pattern: 'ok', color: 'red', caseSensitive: false },
    ]);
    expect(rules).toHaveLength(1);
    expect('PROD-db1'.match(rules[0]?.re ?? /$^/)).toBeTruthy();
    expect(isValidPattern('a+')).toBe(true);
    expect(isValidPattern('(')).toBe(false);
    expect(isValidPattern('')).toBe(false);
  });
});
