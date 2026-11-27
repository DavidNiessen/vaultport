// SPDX-License-Identifier: GPL-3.0-or-later
/**
 * A tiny fake shell used by the browser mock backend so terminal UI can be developed, tested
 * and screenshotted without a real server. It is never part of the Tauri runtime path.
 */
import type { SessionEvent } from './index';

interface MockChannel {
  onmessage: (msg: SessionEvent) => void;
}

interface FakeSession {
  channel: MockChannel;
  line: string;
  host: string;
  user: string;
  cwd: string;
}

const sessions = new Map<string, FakeSession>();
const enc = new TextEncoder();

function b64(s: string): string {
  const bytes = enc.encode(s);
  let bin = '';
  bytes.forEach((b) => (bin += String.fromCharCode(b)));
  return btoa(bin);
}

function out(s: FakeSession, text: string) {
  s.channel.onmessage({ type: 'output', data: b64(text) });
}

function prompt(s: FakeSession) {
  out(s, `\x1b[1;32m${s.user}@${s.host}\x1b[0m:\x1b[1;34m${s.cwd}\x1b[0m$ `);
}

const MOTD = (host: string) =>
  [
    '',
    `\x1b[1mWelcome to ${host}\x1b[0m (GNU/Linux 6.8.0-49-generic x86_64)`,
    '',
    ' * System load:  0.08              Processes:             142',
    ' * Usage of /:   37.4% of 78.15GB  Users logged in:       1',
    ' * Memory usage: 23%               IPv4 address for eth0: 10.20.1.11',
    '',
    '\x1b[33m3 updates can be applied immediately.\x1b[0m',
    '',
  ].join('\r\n');

function run(s: FakeSession, cmd: string): boolean {
  const c = cmd.trim();
  const [name, ...args] = c.split(/\s+/);
  switch (name) {
    case '':
      break;
    case 'help':
      out(
        s,
        'Demo shell. Try: ls, uname -a, uptime, date, whoami, df -h, systemctl status nginx, clear, exit\r\n',
      );
      break;
    case 'ls':
      out(
        s,
        '\x1b[1;34mapp\x1b[0m  \x1b[1;34mbackups\x1b[0m  deploy.sh  \x1b[1;34mlogs\x1b[0m  README.md  \x1b[32mrun.sh\x1b[0m\r\n',
      );
      break;
    case 'whoami':
      out(s, `${s.user}\r\n`);
      break;
    case 'uname':
      out(
        s,
        args.includes('-a')
          ? `Linux ${s.host} 6.8.0-49-generic #49-Ubuntu SMP x86_64 GNU/Linux\r\n`
          : 'Linux\r\n',
      );
      break;
    case 'uptime':
      out(s, ' 14:02:11 up 41 days,  3:12,  1 user,  load average: 0.08, 0.11, 0.09\r\n');
      break;
    case 'date':
      out(s, `${new Date().toString()}\r\n`);
      break;
    case 'df':
      out(
        s,
        'Filesystem      Size  Used Avail Use% Mounted on\r\n/dev/vda1        79G   30G   46G  40% /\r\ntmpfs           3.9G     0  3.9G   0% /dev/shm\r\n',
      );
      break;
    case 'systemctl':
      out(
        s,
        '\x1b[32m●\x1b[0m nginx.service - A high performance web server\r\n     Loaded: loaded (/lib/systemd/system/nginx.service; enabled)\r\n     Active: \x1b[1;32mactive (running)\x1b[0m since Mon 2026-10-16 10:49:58 UTC; 1 month ago\r\n   Main PID: 812 (nginx)\r\n',
      );
      break;
    case 'clear':
      out(s, '\x1b[2J\x1b[H');
      break;
    case 'cd':
      s.cwd = args[0] ?? '~';
      break;
    case 'exit':
    case 'logout':
      out(s, 'logout\r\n');
      return false;
    default:
      out(s, `${name ?? ''}: command not found\r\n`);
  }
  return true;
}

export function mockSessionOpen(
  args: Record<string, unknown>,
  hostLabel: (id: string) => string | undefined = () => undefined,
): string {
  const id = crypto.randomUUID();
  const channel = args.events as MockChannel;
  const req = args.request as {
    target: { type: string; hostId?: string; host?: string; username?: string | null };
  };
  const host =
    req.target.type === 'local'
      ? 'localhost'
      : (req.target.host ?? hostLabel(req.target.hostId ?? '') ?? 'server');
  const s: FakeSession = {
    channel,
    line: '',
    host,
    user: req.target.username ?? 'deploy',
    cwd: '~',
  };
  sessions.set(id, s);
  setTimeout(() => {
    channel.onmessage({ type: 'status', status: 'connected' });
    out(s, MOTD(host));
    out(s, `Last login: ${new Date(Date.now() - 86_400_000).toUTCString()} from 192.168.1.20\r\n`);
    prompt(s);
  }, 350);
  return id;
}

export function mockSessionWrite(args: Record<string, unknown>): null {
  const s = sessions.get(args.id as string);
  if (!s) return null;
  for (const ch of args.data as string) {
    if (ch === '\r') {
      out(s, '\r\n');
      const alive = run(s, s.line);
      s.line = '';
      if (!alive) {
        sessions.delete(args.id as string);
        s.channel.onmessage({ type: 'closed', reason: null, error: false });
        return null;
      }
      prompt(s);
    } else if (ch === '\x7f') {
      if (s.line.length > 0) {
        s.line = s.line.slice(0, -1);
        out(s, '\b \b');
      }
    } else if (ch === '\x03') {
      out(s, '^C\r\n');
      s.line = '';
      prompt(s);
    } else if (ch >= ' ') {
      s.line += ch;
      out(s, ch);
    }
  }
  return null;
}

export function mockSessionClose(args: Record<string, unknown>): null {
  sessions.delete(args.id as string);
  return null;
}

const MOCK_COMMANDS = ['df', 'docker', 'git', 'kubectl', 'ls', 'systemctl', 'uname', 'uptime'];
const MOCK_GIT = ['checkout', 'cherry-pick', 'clone', 'commit', 'pull', 'push', 'status'];

/** A small stand-in for the term-intel completer: commands, then `git` subcommands. */
export function mockTermComplete(args: Record<string, unknown>) {
  const req = args.req as { line: string; cursor: number };
  const before = req.line.slice(0, req.cursor);
  const words = before.split(/\s+/);
  const current = words[words.length - 1] ?? '';
  const pool =
    words.length === 1 ? MOCK_COMMANDS : words.length === 2 && words[0] === 'git' ? MOCK_GIT : [];
  const kind = words.length === 1 ? 'command' : 'subcommand';
  return {
    items: pool
      .filter((w) => w.startsWith(current) && w !== current)
      .map((w) => ({
        text: w,
        label: w,
        description: null,
        kind,
        deleteBefore: [...current].length,
        deleteAfter: 0,
      })),
  };
}
