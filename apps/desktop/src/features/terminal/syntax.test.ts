// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from 'vitest';

import { tokenizeLine, type TokenKind } from './syntax';

const KNOWN = new Set(['git', 'ls', 'grep', 'sudo', 'echo', 'cd', 'env', 'wc']);
const known = (c: string) => KNOWN.has(c);

function kinds(line: string, cursor = -1): [string, TokenKind][] {
  return tokenizeLine(line, known, cursor).map((t) => [line.slice(t.start, t.end), t.kind]);
}

describe('command line syntax', () => {
  it('colours commands, options, arguments and pipes', () => {
    expect(kinds('git commit -m "fix: x" | wc -l')).toEqual([
      ['git', 'command'],
      ['commit', 'argument'],
      ['-m', 'option'],
      ['"fix: x"', 'string'],
      ['|', 'operator'],
      ['wc', 'command'],
      ['-l', 'option'],
    ]);
  });

  it('marks unknown commands only once they are typed', () => {
    expect(kinds('gti status')[0]).toEqual(['gti', 'unknownCommand']);
    expect(kinds('gti', 3)[0]).toEqual(['gti', 'command']);
    // No list: nothing is unknown.
    expect(tokenizeLine('gti', () => null)[0]?.kind).toBe('command');
    // Paths are run as they are.
    expect(kinds('./build.sh')[0]).toEqual(['./build.sh', 'command']);
  });

  it('follows wrappers, assignments and separators', () => {
    expect(kinds('FOO=1 sudo -E gti && ls ~/src')).toEqual([
      ['FOO=', 'assignment'],
      ['1', 'argument'],
      ['sudo', 'command'],
      ['-E', 'option'],
      ['gti', 'unknownCommand'],
      ['&&', 'operator'],
      ['ls', 'command'],
      ['~/src', 'path'],
    ]);
  });

  it('finds variables, also inside double quotes', () => {
    expect(kinds('echo "$HOME/x" $USER \'$no\'')).toEqual([
      ['echo', 'command'],
      ['"', 'string'],
      ['$HOME', 'variable'],
      ['/x"', 'string'],
      ['$USER', 'variable'],
      ["'$no'", 'string'],
    ]);
  });

  it('handles keywords, comments and substitutions', () => {
    expect(kinds('if grep -q x f; then echo $(ls); fi # done')).toEqual([
      ['if', 'keyword'],
      ['grep', 'command'],
      ['-q', 'option'],
      ['x', 'argument'],
      ['f', 'argument'],
      [';', 'operator'],
      ['then', 'keyword'],
      ['echo', 'command'],
      ['$(', 'operator'],
      ['ls', 'command'],
      [')', 'operator'],
      [';', 'operator'],
      ['fi', 'keyword'],
      ['# done', 'comment'],
    ]);
    expect(kinds('cmd --out=file.txt > log 2>&1').map(([, k]) => k)).toEqual([
      'unknownCommand',
      'option',
      'argument',
      'operator',
      'argument',
      'operator',
    ]);
  });

  it('never throws on odd input', () => {
    for (const line of ['"', "'", '$(', '\\', '| |', '$((1+', 'a\\', '${', '((', '#']) {
      expect(() => tokenizeLine(line, known, line.length)).not.toThrow();
    }
  });
});
