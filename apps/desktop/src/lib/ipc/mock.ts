// SPDX-License-Identifier: GPL-3.0-or-later
/**
 * In-memory implementation of the core's commands, used when the UI runs in a plain browser
 * (`pnpm dev` without Tauri, Playwright end-to-end tests and screenshot generation). It is only
 * loaded in development builds or when `VITE_MOCK_IPC=1`.
 */
import { emit as tauriEmit } from '@tauri-apps/api/event';
import { mockIPC, mockWindows } from '@tauri-apps/api/mocks';

import type {
  AgentApprovalRequest,
  AuditEntry,
  Group,
  Host,
  Identity,
  KeyMeta,
  KnownHost,
  PlatformInfo,
  PortForwardRule,
  Snippet,
  SnippetPackage,
  VaultStatus,
  WorkspaceTemplate,
  HostConfig,
} from './index';
import { emptyConfig } from './index';
import {
  MOCK_COMMAND_NAMES,
  MOCK_HISTORY,
  mockSessionClose,
  mockSessionOpen,
  mockSessionWrite,
  mockTermComplete,
} from './mockShell';

type Handler = (args: Record<string, unknown>) => unknown;

interface MockDb {
  hosts: Map<string, Host>;
  groups: Map<string, Group>;
  identities: Map<string, Identity>;
  keys: Map<string, KeyMeta>;
  snippets: Map<string, Snippet>;
  packages: Map<string, SnippetPackage>;
  forwards: Map<string, PortForwardRule>;
  knownHosts: Map<string, KnownHost>;
  workspaces: Map<string, WorkspaceTemplate>;
  settings: Map<string, unknown>;
  secrets: Map<string, string>;
}

const uuid = (n: number) => `0190f0e0-0000-7000-8000-${n.toString(16).padStart(12, '0')}`;

function seed(): MockDb {
  const db: MockDb = {
    hosts: new Map(),
    groups: new Map(),
    identities: new Map(),
    keys: new Map(),
    snippets: new Map(),
    packages: new Map(),
    forwards: new Map(),
    knownHosts: new Map(),
    workspaces: new Map(),
    settings: new Map(),
    secrets: new Map(),
  };
  const now = Date.now();
  const cfg = (): HostConfig => emptyConfig();
  const groups: Group[] = [
    {
      id: uuid(1),
      label: 'Production',
      parent: null,
      integration: null,
      config: { ...cfg(), terminal: { ...cfg().terminal, theme: 'ember' } },
      createdAt: now,
      updatedAt: now,
    },
    {
      id: uuid(2),
      label: 'Staging',
      parent: null,
      integration: null,
      config: cfg(),
      createdAt: now,
      updatedAt: now,
    },
    {
      id: uuid(3),
      label: 'EU West',
      parent: uuid(1),
      integration: null,
      config: cfg(),
      createdAt: now,
      updatedAt: now,
    },
    {
      id: uuid(4),
      label: 'Home lab',
      parent: null,
      integration: null,
      config: cfg(),
      createdAt: now,
      updatedAt: now,
    },
  ];
  groups.forEach((g) => db.groups.set(g.id, g));
  const mk = (
    n: number,
    label: string,
    address: string,
    group: string | null,
    tags: string[],
    os: Host['os'],
  ): Host => ({
    id: uuid(100 + n),
    label,
    address,
    group,
    tags,
    config: {
      ...cfg(),
      ssh: {
        ...cfg().ssh,
        credential: { type: 'inline', username: 'deploy', password: null, keys: [] },
      },
    },
    notes: null,
    cloudId: null,
    os,
    favorite: n === 1,
    createdAt: now,
    updatedAt: now,
    lastConnectedAt: n < 4 ? now - n * 3_600_000 : null,
  });
  [
    mk(1, 'api-prod-eu-1', '10.20.1.11', uuid(3), ['api', 'k8s'], 'ubuntu'),
    mk(2, 'db-prod-eu-1', '10.20.2.21', uuid(3), ['postgres'], 'debian'),
    mk(3, 'bastion.example.com', 'bastion.example.com', uuid(1), ['jump'], 'alpine'),
    mk(4, 'web-staging', '172.16.0.5', uuid(2), ['web', 'nginx'], 'rocky'),
    mk(5, 'nas', '192.168.1.10', uuid(4), ['storage'], 'truenas'),
    mk(6, 'raspberry-pi', '192.168.1.42', uuid(4), ['iot'], 'raspbian'),
    mk(7, 'router', '192.168.1.1', uuid(4), [], 'openwrt'),
  ].forEach((h) => db.hosts.set(h.id, h));
  db.identities.set(uuid(200), {
    id: uuid(200),
    label: 'Deploy user',
    username: 'deploy',
    password: null,
    keys: [uuid(300)],
    createdAt: now,
    updatedAt: now,
  });
  db.keys.set(uuid(300), {
    id: uuid(300),
    label: 'Work laptop',
    algorithm: 'ed25519',
    publicKey:
      'ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIGq4n0xvYhJ0pZ2c3QeYV4s3r5Qm2vXk9yXh1Lw6aPpB work-laptop',
    fingerprint: 'SHA256:Vb3fB7d0Jx2nQ1mUo4c3YzHq8sKf5tWp9rLe6aD2gXc',
    storage: { type: 'vault', secret: uuid(301) },
    encrypted: true,
    bits: null,
    passphrase: null,
    certificate: null,
    createdAt: now,
    updatedAt: now,
  });
  db.packages.set(uuid(400), { id: uuid(400), label: 'Maintenance', createdAt: now });
  [
    {
      id: uuid(401),
      label: 'Disk usage',
      script: 'df -h && du -sh /var/log/* | sort -h | tail',
      package: uuid(400),
    },
    {
      id: uuid(402),
      label: 'Restart service',
      script: 'sudo systemctl restart {{service:nginx}}',
      package: uuid(400),
    },
    { id: uuid(403), label: 'Tail syslog', script: 'sudo journalctl -f -n 200', package: null },
  ].forEach((s) =>
    db.snippets.set(s.id, {
      ...s,
      tags: [],
      targets: [],
      runCount: 0,
      createdAt: now,
      updatedAt: now,
    }),
  );
  db.forwards.set(uuid(500), {
    id: uuid(500),
    label: 'Postgres (prod)',
    kind: 'local',
    host: uuid(103),
    bindAddress: '127.0.0.1',
    bindPort: 5433,
    destHost: '10.20.2.21',
    destPort: 5432,
    autoStart: false,
    createdAt: now,
    updatedAt: now,
  });
  db.knownHosts.set(uuid(600), {
    id: uuid(600),
    pattern: 'bastion.example.com',
    keyType: 'ssh-ed25519',
    publicKey: 'AAAAC3NzaC1lZDI1NTE5AAAAIOMqqnkVzrm0SdG6UOoqKLsabgH5C9okWi0dh2l9GKJl',
    fingerprint: 'SHA256:+DiY3wvvV6TuJJhbpZisF/zLDA0zPMSvHdkr4UvCOqU',
    marker: null,
    comment: null,
    addedAt: now,
  });
  return db;
}

let db = seed();
let vault: VaultStatus = {
  vaults: ['personal'],
  active: null,
  locked: true,
  hasPassword: true,
  hasDeviceKey: false,
};

function platform(): PlatformInfo {
  const params = new URLSearchParams(window.location.search);
  const forced = params.get('platform') as PlatformInfo['kind'] | null;
  const ua = navigator.userAgent;
  const kind: PlatformInfo['kind'] =
    forced ??
    (ua.includes('iPhone') || ua.includes('iPad')
      ? 'ios'
      : ua.includes('Android')
        ? 'android'
        : ua.includes('Mac')
          ? 'macos'
          : ua.includes('Win')
            ? 'windows'
            : 'linux');
  const mobile = kind === 'ios' || kind === 'android';
  return {
    kind,
    mobile,
    arch: 'x86_64',
    version: '0.1.0',
    decorationLayout: params.get('layout'),
    nativeDecorations: mobile,
    localTerminal: !mobile,
  };
}

function crud<T extends { id: string }>(map: () => Map<string, T>, kind: string) {
  return {
    list: () => [...map().values()],
    save: (args: Record<string, unknown>) => {
      const e = args.entity as T;
      map().set(e.id, e);
      emit(kind);
      return e;
    },
    remove: (args: Record<string, unknown>) => {
      const ok = map().delete(args.id as string);
      emit(kind);
      return ok;
    },
  };
}

function emit(kind: string) {
  void tauriEmit('vault://changed', { kind });
}

function requireUnlocked() {
  if (vault.locked) throw { code: 'locked', message: 'the vault is locked' };
}

const hosts = crud(() => db.hosts, 'host');
const groups = crud(() => db.groups, 'group');
const identities = crud(() => db.identities, 'identity');
const keys = crud(() => db.keys, 'key');
const snippets = crud(() => db.snippets, 'snippet');
const packages = crud(() => db.packages, 'package');
const forwards = crud(() => db.forwards, 'forward');
const knownHosts = crud(() => db.knownHosts, 'knownHost');
const workspaces = crud(() => db.workspaces, 'workspace');

const handlers: Record<string, Handler> = {
  platform_info: () => platform(),
  vault_status: () => vault,
  vault_create: (a) => {
    const req = a.request as { name: string };
    db = seed();
    vault = {
      vaults: [...new Set([...vault.vaults, req.name])],
      active: req.name,
      locked: false,
      hasPassword: true,
      hasDeviceKey: false,
    };
    return vault;
  },
  vault_unlock: (a) => {
    if (a.password !== null && a.password !== 'password') {
      throw {
        code: 'authentication',
        message: 'wrong password or key, or the data has been tampered with',
      };
    }
    vault = { ...vault, active: a.name as string, locked: false };
    return vault;
  },
  vault_lock: () => (vault = { ...vault, locked: true }),
  vault_set_password: () => (vault = { ...vault, hasPassword: true }),
  vault_remove_password: () => (vault = { ...vault, hasPassword: false }),
  vault_set_device_unlock: (a) => (vault = { ...vault, hasDeviceKey: Boolean(a.enabled) }),
  list_hosts: hosts.list,
  save_host: hosts.save,
  delete_host: hosts.remove,
  resolve_host: (a) => {
    const h = db.hosts.get(a.id as string);
    if (!h) throw { code: 'notFound', message: 'host not found' };
    return {
      host: h.id,
      address: h.address,
      config: { ...h.config, ssh: { ...h.config.ssh, port: h.config.ssh.port ?? 22 } },
      ancestry: [],
    };
  },
  list_groups: groups.list,
  save_group: groups.save,
  delete_group: groups.remove,
  list_identities: identities.list,
  save_identity: identities.save,
  delete_identity: identities.remove,
  list_keys: keys.list,
  save_key_meta: keys.save,
  delete_key_meta: keys.remove,
  list_snippets: snippets.list,
  save_snippet: snippets.save,
  delete_snippet: snippets.remove,
  list_packages: packages.list,
  save_package: packages.save,
  delete_package: packages.remove,
  list_forwards: forwards.list,
  save_forward: forwards.save,
  delete_forward: forwards.remove,
  list_known_hosts: knownHosts.list,
  save_known_host: knownHosts.save,
  delete_known_host: knownHosts.remove,
  list_workspaces: workspaces.list,
  save_workspace: workspaces.save,
  delete_workspace: workspaces.remove,
  store_secret: (a) => {
    const id = (a.existing as string | null) ?? crypto.randomUUID();
    db.secrets.set(id, a.value as string);
    return id;
  },
  delete_secret: (a) => db.secrets.delete(a.id as string),
  session_open: (a) => mockSessionOpen(a, (id) => db.hosts.get(id)?.label),
  session_write: mockSessionWrite,
  session_resize: () => null,
  session_close: mockSessionClose,
  session_respond: () => null,
  serial_list_ports: () => [{ path: '/dev/ttyUSB0', description: 'FTDI USB Serial' }],
  prompt_respond: () => null,
  list_logs: () => [],
  forward_status: () => [],
  transfer_list: () => [],
  sync_list_targets: () => [],
  history_list: () => MOCK_HISTORY.map((command, i) => ({ command, count: 12 - i * 3 })),
  term_complete: mockTermComplete,
  term_commands: () => ({ names: MOCK_COMMAND_NAMES, complete: true }),
  term_integration_bootstrap: () => ({
    start: ' true\r',
    ready: 'VaultportReady',
    bash: [],
    zsh: [],
  }),
  ai_generate_command: (a) => ({
    command: `find . -type f -size +100M  # ${String(a.prompt).slice(0, 40)}`,
    explanation: 'Lists files larger than 100 MB below the current directory.',
    risk: 'low',
  }),
  ai_fix_command: () => ({
    command: 'git push --set-upstream origin main',
    explanation: 'The branch has no upstream yet; set it while pushing.',
    risk: 'low',
  }),
  ai_status: () => ({
    state: 'notInstalled',
    modelName: 'Qwen2.5-Coder-1.5B-Instruct (Q4_K_M)',
    license: 'Apache-2.0',
    downloaded: 0,
    total: 1117320768,
    error: null,
  }),
  default_ssh_config_path: () => null,
  agent_status: () => agentStatus(),
  agent_save_settings: (a) => {
    agent.settings = { ...agent.settings, ...(a.settings as typeof agent.settings) };
    return agentStatus();
  },
  agent_respond: (a) => {
    const id = a.id as string;
    const pending = agent.pending.get(id);
    if (!pending) throw { code: 'notFound', message: 'the request is no longer pending' };
    agent.pending.delete(id);
    agent.audit.unshift({
      id: crypto.randomUUID(),
      at: Date.now(),
      client: pending.client,
      tool: pending.tool,
      host: pending.host,
      summary: pending.details,
      outcome: a.decision === 'deny' ? 'denied' : 'ok',
      message: null,
    });
    void tauriEmit('agent://audit', null);
    return null;
  },
  agent_audit_log: () => [...agent.audit],
  agent_rotate_token: () => agentStatus(),
  agent_disconnect_all: () => null,
  get_settings: (a) => db.settings.get(a.name as string) ?? null,
  save_settings: (a) => {
    db.settings.set(a.name as string, a.value);
    emit('settings');
    return null;
  },
};

const agent = {
  settings: {
    enabled: false,
    approval: 'always' as 'always' | 'changesOnly',
    readOnly: false,
    allHosts: true,
    hosts: [] as string[],
  },
  pending: new Map<string, AgentApprovalRequest>(),
  audit: [] as AuditEntry[],
};

function agentStatus() {
  const command = '/usr/bin/vaultport';
  const desktop = JSON.stringify(
    { mcpServers: { vaultport: { command, args: ['mcp'] } } },
    null,
    2,
  );
  return {
    supported: true,
    settings: agent.settings,
    listening: agent.settings.enabled,
    error: null,
    clients: [],
    command,
    configs: [
      {
        client: 'claude-code',
        kind: 'shell',
        location: null,
        text: `claude mcp add --scope user vaultport -- ${command} mcp`,
      },
      {
        client: 'claude-desktop',
        kind: 'json',
        location: 'claude_desktop_config.json',
        text: desktop,
      },
      { client: 'generic', kind: 'shell', location: null, text: `${command} mcp` },
    ],
  };
}

/** Simulates an agent asking for approval (dev and end-to-end tests). */
function mockAgentRequest(details = 'systemctl restart nginx'): string {
  const req: AgentApprovalRequest = {
    id: crypto.randomUUID(),
    client: 'Claude Code 2.1',
    tool: 'run_command',
    title: 'Run command',
    host: 'web-01',
    details,
    destructive: true,
  };
  agent.pending.set(req.id, req);
  void tauriEmit('agent://approval', req);
  return req.id;
}

const UNLOCKED_ONLY = /^(list_|save_|delete_|resolve_|store_|get_settings|sync_|history_)/;

/** Registers an additional mock handler (used by feature modules, e.g. terminal sessions). */
export function registerMockHandler(cmd: string, handler: Handler): void {
  handlers[cmd] = handler;
}

export function installMockIpc(): void {
  mockWindows('main');
  (window as unknown as { __mockAgentRequest: typeof mockAgentRequest }).__mockAgentRequest =
    mockAgentRequest;
  mockIPC(
    (cmd, payload) => {
      const args = (payload ?? {}) as Record<string, unknown>;
      if (cmd.startsWith('plugin:')) return null;
      const h = handlers[cmd];
      if (!h) throw { code: 'unsupported', message: `mock: ${cmd} is not implemented` };
      if (UNLOCKED_ONLY.test(cmd)) requireUnlocked();
      return h(args);
    },
    { shouldMockEvents: true },
  );
}
