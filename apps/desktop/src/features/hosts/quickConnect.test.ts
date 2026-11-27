// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from 'vitest';

import { parseQuickConnect } from './quickConnect';

describe('parseQuickConnect', () => {
  it('parses ssh command lines', () => {
    expect(parseQuickConnect('ssh root@10.0.0.1 -p 2222')).toEqual({
      username: 'root',
      host: '10.0.0.1',
      port: 2222,
      jump: null,
    });
    expect(parseQuickConnect('ssh -l admin -J bastion example.com')).toMatchObject({
      username: 'admin',
      host: 'example.com',
      jump: 'bastion',
    });
  });

  it('parses bare targets', () => {
    expect(parseQuickConnect('me@host.example:2022')).toMatchObject({ port: 2022 });
    expect(parseQuickConnect('deploy@[fd00::1]:22')).toMatchObject({ host: 'fd00::1', port: 22 });
  });

  it('ignores plain search text', () => {
    expect(parseQuickConnect('web')).toBeNull();
    expect(parseQuickConnect('prod db')).toBeNull();
    expect(parseQuickConnect('ssh -p 70000 host')).toBeNull();
    expect(parseQuickConnect('ssh')).toBeNull();
  });
});
