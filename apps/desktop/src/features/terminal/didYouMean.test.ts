// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from 'vitest';

import { closestCommand, editDistance, fixCommandLine, isNotFound } from './didYouMean';

const KNOWN = ['git', 'grep', 'docker', 'kubectl', 'python3', 'ls', 'sl', 'systemctl', 'gzip'];

describe('did you mean', () => {
  it('measures edits with swaps', () => {
    expect(editDistance('gti', 'git')).toBe(1);
    expect(editDistance('dokcer', 'docker')).toBe(1);
    expect(editDistance('kubeclt', 'kubectl')).toBe(1);
    expect(editDistance('abc', 'xyz', 2)).toBe(3);
  });

  it('finds the closest installed command', () => {
    expect(closestCommand('gti', KNOWN)).toBe('git');
    expect(closestCommand('dokcer', KNOWN)).toBe('docker');
    expect(closestCommand('systemclt', KNOWN)).toBe('systemctl');
    expect(closestCommand('pyhton3', KNOWN)).toBe('python3');
    expect(closestCommand('zzzz', KNOWN)).toBeNull();
    expect(closestCommand('./run.sh', KNOWN)).toBeNull();
    expect(closestCommand('x', KNOWN)).toBeNull();
  });

  it('rewrites only the command word', () => {
    expect(fixCommandLine('gti push -f', 'git')).toBe('git push -f');
    expect(fixCommandLine('FOO=1 gti st', 'git')).toBe('FOO=1 git st');
    expect(isNotFound(127, '')).toBe(true);
    expect(isNotFound(1, 'bash: gti: command not found')).toBe(true);
    expect(isNotFound(1, 'error')).toBe(false);
  });
});
