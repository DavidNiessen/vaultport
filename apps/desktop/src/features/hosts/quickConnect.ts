// SPDX-License-Identifier: GPL-3.0-or-later
/**
 * Client-side mirror of `core_model::parse_quick_connect` used for instant feedback while
 * typing. The core re-validates everything before connecting.
 */
export interface QuickConnect {
  username: string | null;
  host: string;
  port: number | null;
  jump: string | null;
}

const HOST_RE =
  /^(?=.{1,253}$)([A-Za-z0-9_](?:[A-Za-z0-9_-]{0,61}[A-Za-z0-9_])?)(\.[A-Za-z0-9_](?:[A-Za-z0-9_-]{0,61}[A-Za-z0-9_])?)*\.?$/;
const IPV6_RE = /^[0-9a-fA-F:]+(%[\w.]+)?$/;

function validHost(h: string): boolean {
  return HOST_RE.test(h) || (h.includes(':') && IPV6_RE.test(h));
}

function parsePort(s: string): number | null {
  if (!/^\d{1,5}$/.test(s)) return null;
  const n = Number(s);
  return n >= 1 && n <= 65535 ? n : null;
}

function parseTarget(t: string): QuickConnect | null {
  const at = t.lastIndexOf('@');
  const username = at > 0 ? t.slice(0, at) : null;
  const hp = at >= 0 ? t.slice(at + 1) : t;
  let host = hp;
  let port: number | null = null;
  const v6 = /^\[([^\]]+)\](?::(\d+))?$/.exec(hp);
  if (v6) {
    host = v6[1] ?? '';
    if (v6[2] !== undefined) {
      port = parsePort(v6[2]);
      if (port === null) return null;
    }
  } else if ((hp.match(/:/g) ?? []).length === 1) {
    const [h, p] = hp.split(':');
    host = h ?? '';
    port = parsePort(p ?? '');
    if (port === null) return null;
  }
  if (!validHost(host)) return null;
  return { username, host, port, jump: null };
}

/** Returns a connection target if `input` looks like `ssh user@host -p 22` / `user@host:22`. */
export function parseQuickConnect(input: string): QuickConnect | null {
  const s = input.trim();
  if (!s) return null;
  if (s.startsWith('ssh://')) return parseTarget(s.slice(6).replace(/\/$/, ''));
  const tokens = s.split(/\s+/);
  if (tokens[0] !== 'ssh') {
    if (tokens.length === 1 && /[@:]/.test(s)) return parseTarget(s);
    return null;
  }
  let port: number | null = null;
  let login: string | null = null;
  let jump: string | null = null;
  let target: string | null = null;
  for (let i = 1; i < tokens.length; i++) {
    const a = tokens[i] ?? '';
    if (['-p', '-l', '-J', '-i', '-o', '-F'].includes(a)) {
      const v = tokens[i + 1];
      if (v === undefined) return null;
      if (a === '-p') {
        port = parsePort(v);
        if (port === null) return null;
      } else if (a === '-l') login = v;
      else if (a === '-J') jump = v;
      i++;
    } else if (/^-p\d+$/.test(a)) {
      port = parsePort(a.slice(2));
    } else if (a.startsWith('-')) {
      continue;
    } else if (!target) {
      target = a;
    } else {
      break;
    }
  }
  if (!target) return null;
  const qc = parseTarget(target);
  if (!qc) return null;
  return { ...qc, username: qc.username ?? login, port: port ?? qc.port, jump };
}
