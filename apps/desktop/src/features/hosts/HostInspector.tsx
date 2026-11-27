// SPDX-License-Identifier: GPL-3.0-or-later
import { ArrowDown, ArrowUp, Play, Plus, Trash2, X } from 'lucide-react';
import { useState, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';

import { OsIcon } from '@/components/OsIcon';
import { Button } from '@/components/ui/button';
import { Select, Switch, TagInput } from '@/components/ui/controls';
import { Field, Input, PasswordInput, Textarea } from '@/components/ui/input';
import { Card, Row } from '@/components/ui/misc';
import { DropdownMenu } from '@/components/ui/overlay';
import {
  commands,
  type Credential,
  type Group,
  type Host,
  type HostConfig,
  type ProxyConfig,
} from '@/lib/ipc';
import { EMPTY } from '@/lib/empty';
import { useBreakpoint } from '@/lib/platform';
import {
  useDeleteHost,
  useGroups,
  useHosts,
  useIdentities,
  useKeys,
  useSaveHost,
  useSnippets,
} from '@/lib/queries';
import { useApp } from '@/stores/app';

import { TERMINAL_THEMES } from '../terminal/themes';
import { useSessions } from '../terminal/sessions';
import { allTags, groupPath } from './model';
import { useAutoSave } from './useAutoSave';

function groupLabel(groups: Group[], id: string): string {
  return groupPath(groups, id)
    .map((g) => g.label)
    .join(' / ');
}

export function GroupSelect({
  value,
  onChange,
  exclude,
  id,
}: {
  value: string | null | undefined;
  onChange: (v: string | null) => void;
  exclude?: string;
  id?: string;
}) {
  const { t } = useTranslation();
  const groups = useGroups().data ?? EMPTY;
  return (
    <Select id={id} value={value ?? ''} onChange={(e) => onChange(e.target.value || null)}>
      <option value="">{t('hosts.noGroup')}</option>
      {groups
        .filter((g) => g.id !== exclude && !groupPath(groups, g.id).some((x) => x.id === exclude))
        .map((g) => ({ g, label: groupLabel(groups, g.id) }))
        .sort((a, b) => a.label.localeCompare(b.label))
        .map(({ g, label }) => (
          <option key={g.id} value={g.id}>
            {label}
          </option>
        ))}
    </Select>
  );
}

/** Editable SSH/protocol settings shared by hosts and groups. */
export function ConfigEditor({
  config,
  onChange,
  selfId,
  inheritedHint,
}: {
  config: HostConfig;
  onChange: (fn: (c: HostConfig) => HostConfig) => void;
  selfId: string;
  inheritedHint?: boolean;
}) {
  const { t } = useTranslation();
  const identities = useIdentities().data ?? EMPTY;
  const keys = useKeys().data ?? EMPTY;
  const hosts = useHosts().data ?? EMPTY;
  const snippets = useSnippets().data ?? EMPTY;
  const [showMore, setShowMore] = useState(false);
  const [password, setPassword] = useState('');
  const ssh = config.ssh;
  const cred: Credential | null = ssh.credential;
  const inline = cred?.type === 'inline' ? cred : null;
  const setSsh = (patch: Partial<HostConfig['ssh']>) => {
    onChange((c) => ({ ...c, ssh: { ...c.ssh, ...patch } }));
  };
  const setInline = (patch: Partial<Extract<Credential, { type: 'inline' }>>) => {
    setSsh({
      credential: {
        type: 'inline',
        username: inline?.username ?? null,
        password: inline?.password ?? null,
        keys: inline?.keys ?? [],
        ...patch,
      },
    });
  };
  const savePassword = async () => {
    if (!password) return;
    const ref = await commands.storeSecret(password, inline?.password ?? null, false);
    setPassword('');
    setInline({ password: ref });
  };
  const chain = ssh.jumpChain ?? [];
  const proxy = ssh.proxy ?? null;
  const setProxy = (p: ProxyConfig | null) => {
    setSsh({ proxy: p });
  };
  const placeholder = inheritedHint ? t('hosts.inherited') : undefined;

  return (
    <>
      <Card title={t('hosts.ssh')}>
        <div className="grid grid-cols-[1fr_96px] gap-3">
          <Field label={t('hosts.username')}>
            {(id) =>
              cred?.type === 'identity' ? (
                <Select
                  id={id}
                  value={cred.id}
                  onChange={(e) => {
                    if (e.target.value === '__inline') setInline({});
                    else setSsh({ credential: { type: 'identity', id: e.target.value } });
                  }}
                >
                  {identities.map((i) => (
                    <option key={i.id} value={i.id}>
                      {i.label} ({i.username})
                    </option>
                  ))}
                  <option value="__inline">{t('hosts.customCredentials')}</option>
                </Select>
              ) : (
                <div className="flex gap-1.5">
                  <Input
                    id={id}
                    value={inline?.username ?? ''}
                    placeholder={placeholder ?? 'root'}
                    onChange={(e) => setInline({ username: e.target.value || null })}
                  />
                  {identities.length > 0 ? (
                    <DropdownMenu
                      trigger={
                        <Button size="icon" variant="ghost" aria-label={t('hosts.useIdentity')}>
                          <Plus className="size-4" />
                        </Button>
                      }
                      items={identities.map((i) => ({
                        label: `${i.label} (${i.username})`,
                        onSelect: () => setSsh({ credential: { type: 'identity', id: i.id } }),
                      }))}
                    />
                  ) : null}
                </div>
              )
            }
          </Field>
          <Field label={t('hosts.port')}>
            {(id) => (
              <Input
                id={id}
                inputMode="numeric"
                value={ssh.port ?? ''}
                placeholder={placeholder ?? '22'}
                onChange={(e) => {
                  const n = Number(e.target.value);
                  setSsh({
                    port:
                      e.target.value === ''
                        ? null
                        : Number.isInteger(n) && n > 0 && n < 65536
                          ? n
                          : (ssh.port ?? null),
                  });
                }}
              />
            )}
          </Field>
        </div>
        {cred?.type !== 'identity' && (
          <div className="mt-3 grid gap-3">
            <Field
              label={t('hosts.password')}
              hint={inline?.password ? t('hosts.passwordStored') : t('hosts.passwordHint')}
            >
              {(id) => (
                <PasswordInput
                  id={id}
                  value={password}
                  placeholder={inline?.password ? '••••••••' : ''}
                  onChange={(e) => setPassword(e.target.value)}
                  onBlur={() => void savePassword()}
                />
              )}
            </Field>
            <Field label={t('hosts.keys')}>
              {() => (
                <div className="flex flex-wrap gap-1.5">
                  {(inline?.keys ?? []).map((kid) => {
                    const k = keys.find((x) => x.id === kid);
                    return (
                      <span
                        key={kid}
                        className="bg-surface-3 inline-flex h-7 items-center gap-1 rounded-full pr-1 pl-3 text-xs"
                      >
                        {k?.label ?? t('hosts.missingKey')}
                        <button
                          type="button"
                          aria-label={t('common.remove')}
                          className="text-subtle hover:text-danger"
                          onClick={() =>
                            setInline({ keys: (inline?.keys ?? []).filter((x) => x !== kid) })
                          }
                        >
                          <X className="size-3.5" />
                        </button>
                      </span>
                    );
                  })}
                  <DropdownMenu
                    align="start"
                    trigger={
                      <Button size="sm" variant="ghost">
                        <Plus className="size-4" />
                        {t('hosts.addKey')}
                      </Button>
                    }
                    items={
                      keys.length === 0
                        ? [{ label: t('hosts.noKeys'), disabled: true }]
                        : keys
                            .filter((k) => !(inline?.keys ?? []).includes(k.id))
                            .map((k) => ({
                              label: k.label,
                              onSelect: () => setInline({ keys: [...(inline?.keys ?? []), k.id] }),
                            }))
                    }
                  />
                </div>
              )}
            </Field>
          </div>
        )}
        <button
          type="button"
          className="text-accent-text mt-4 text-[13px] font-medium"
          onClick={() => setShowMore((v) => !v)}
        >
          {showMore ? t('hosts.showLess') : t('hosts.showMore')}
        </button>
        {showMore && (
          <div className="mt-3 grid gap-4">
            <Field label={t('hosts.jumpHosts')} hint={t('hosts.jumpHostsHint')}>
              {() => (
                <div className="flex flex-col gap-1.5">
                  {chain.map((hid, i) => {
                    const h = hosts.find((x) => x.id === hid);
                    return (
                      <div
                        key={`${hid}-${String(i)}`}
                        className="bg-surface-3 flex h-9 items-center gap-2 rounded-lg px-2"
                      >
                        <span className="text-subtle w-4 text-xs">{i + 1}</span>
                        <span className="flex-1 truncate text-[13px]">
                          {h?.label ?? t('hosts.missingHost')}
                        </span>
                        <Button
                          size="icon-sm"
                          variant="ghost"
                          aria-label={t('common.moveUp')}
                          disabled={i === 0}
                          onClick={() => {
                            const c = [...chain];
                            [c[i - 1], c[i]] = [c[i] ?? '', c[i - 1] ?? ''];
                            setSsh({ jumpChain: c });
                          }}
                        >
                          <ArrowUp className="size-3.5" />
                        </Button>
                        <Button
                          size="icon-sm"
                          variant="ghost"
                          aria-label={t('common.moveDown')}
                          disabled={i === chain.length - 1}
                          onClick={() => {
                            const c = [...chain];
                            [c[i + 1], c[i]] = [c[i] ?? '', c[i + 1] ?? ''];
                            setSsh({ jumpChain: c });
                          }}
                        >
                          <ArrowDown className="size-3.5" />
                        </Button>
                        <Button
                          size="icon-sm"
                          variant="ghost"
                          aria-label={t('common.remove')}
                          onClick={() => setSsh({ jumpChain: chain.filter((_, j) => j !== i) })}
                        >
                          <X className="size-3.5" />
                        </Button>
                      </div>
                    );
                  })}
                  <DropdownMenu
                    align="start"
                    trigger={
                      <Button size="sm" variant="ghost" className="self-start">
                        <Plus className="size-4" />
                        {t('hosts.addJumpHost')}
                      </Button>
                    }
                    items={hosts
                      .filter((h) => h.id !== selfId && !chain.includes(h.id))
                      .map((h) => ({
                        label: h.label,
                        onSelect: () => setSsh({ jumpChain: [...chain, h.id] }),
                      }))}
                  />
                </div>
              )}
            </Field>

            <Field label={t('hosts.proxy')}>
              {(id) => (
                <div className="grid gap-2">
                  <Select
                    id={id}
                    value={proxy?.kind ?? ''}
                    onChange={(e) => {
                      const kind = e.target.value as '' | 'http' | 'socks5';
                      setProxy(
                        kind
                          ? {
                              kind,
                              host: proxy?.host ?? '',
                              port: proxy?.port ?? (kind === 'http' ? 3128 : 1080),
                              username: proxy?.username ?? null,
                              password: proxy?.password ?? null,
                            }
                          : null,
                      );
                    }}
                  >
                    <option value="">{t('hosts.noProxy')}</option>
                    <option value="socks5">SOCKS5</option>
                    <option value="http">HTTP CONNECT</option>
                  </Select>
                  {proxy ? (
                    <div className="grid grid-cols-[1fr_88px] gap-2">
                      <Input
                        aria-label={t('hosts.proxyHost')}
                        placeholder={t('hosts.proxyHost')}
                        value={proxy.host}
                        onChange={(e) => setProxy({ ...proxy, host: e.target.value })}
                      />
                      <Input
                        aria-label={t('hosts.port')}
                        inputMode="numeric"
                        value={proxy.port}
                        onChange={(e) =>
                          setProxy({ ...proxy, port: Number(e.target.value) || proxy.port })
                        }
                      />
                      <Input
                        aria-label={t('hosts.username')}
                        placeholder={t('hosts.username')}
                        className="col-span-2"
                        value={proxy.username ?? ''}
                        onChange={(e) => setProxy({ ...proxy, username: e.target.value || null })}
                      />
                    </div>
                  ) : null}
                </div>
              )}
            </Field>

            <div>
              <Row label={t('hosts.agentForwarding')} description={t('hosts.agentForwardingHint')}>
                <Switch
                  label={t('hosts.agentForwarding')}
                  checked={ssh.agentForwarding ?? false}
                  onCheckedChange={(v) => setSsh({ agentForwarding: v })}
                />
              </Row>
              <Row label={t('hosts.postQuantum')} description={t('hosts.postQuantumHint')}>
                <Switch
                  label={t('hosts.postQuantum')}
                  checked={ssh.postQuantum ?? true}
                  onCheckedChange={(v) => setSsh({ postQuantum: v })}
                />
              </Row>
              <Row label={t('hosts.legacy')} description={t('hosts.legacyHint')}>
                <Switch
                  label={t('hosts.legacy')}
                  checked={ssh.legacyAlgorithms ?? false}
                  onCheckedChange={(v) => setSsh({ legacyAlgorithms: v })}
                />
              </Row>
              <Row label={t('hosts.compression')}>
                <Switch
                  label={t('hosts.compression')}
                  checked={ssh.compression ?? false}
                  onCheckedChange={(v) => setSsh({ compression: v })}
                />
              </Row>
              <Row label={t('hosts.strictHostKey')} description={t('hosts.strictHostKeyHint')}>
                <Switch
                  label={t('hosts.strictHostKey')}
                  checked={ssh.hostKeyPolicy === 'strict'}
                  onCheckedChange={(v) => setSsh({ hostKeyPolicy: v ? 'strict' : 'ask' })}
                />
              </Row>
            </div>

            <Field label={t('hosts.keepalive')}>
              {(id) => (
                <Input
                  id={id}
                  inputMode="numeric"
                  value={ssh.keepaliveSecs ?? ''}
                  placeholder={placeholder ?? '30'}
                  onChange={(e) =>
                    setSsh({
                      keepaliveSecs: e.target.value === '' ? null : Number(e.target.value) || 0,
                    })
                  }
                />
              )}
            </Field>

            <Field label={t('hosts.startupSnippet')}>
              {(id) => (
                <Select
                  id={id}
                  value={ssh.startupSnippet ?? ''}
                  onChange={(e) => setSsh({ startupSnippet: e.target.value || null })}
                >
                  <option value="">{t('common.none')}</option>
                  {snippets.map((s) => (
                    <option key={s.id} value={s.id}>
                      {s.label}
                    </option>
                  ))}
                </Select>
              )}
            </Field>

            <Field label={t('hosts.envVars')} hint={t('hosts.envVarsHint')}>
              {() => (
                <div className="flex flex-col gap-1.5">
                  {(ssh.env ?? []).map((v, i) => (
                    <div key={String(i)} className="grid grid-cols-[1fr_1fr_auto] gap-1.5">
                      <Input
                        aria-label={t('hosts.envName')}
                        placeholder="NAME"
                        value={v.name}
                        onChange={(e) => {
                          const env = [...(ssh.env ?? [])];
                          env[i] = { ...v, name: e.target.value.replace(/[^A-Za-z0-9_]/g, '') };
                          setSsh({ env });
                        }}
                      />
                      <Input
                        aria-label={t('hosts.envValue')}
                        placeholder="value"
                        value={v.value}
                        onChange={(e) => {
                          const env = [...(ssh.env ?? [])];
                          env[i] = { ...v, value: e.target.value };
                          setSsh({ env });
                        }}
                      />
                      <Button
                        size="icon"
                        variant="ghost"
                        aria-label={t('common.remove')}
                        onClick={() => setSsh({ env: (ssh.env ?? []).filter((_, j) => j !== i) })}
                      >
                        <Trash2 className="size-4" />
                      </Button>
                    </div>
                  ))}
                  <Button
                    size="sm"
                    variant="ghost"
                    className="self-start"
                    onClick={() => setSsh({ env: [...(ssh.env ?? []), { name: '', value: '' }] })}
                  >
                    <Plus className="size-4" />
                    {t('hosts.addVariable')}
                  </Button>
                </div>
              )}
            </Field>
          </div>
        )}
      </Card>

      <Card title={t('hosts.mosh')}>
        <Row label={t('hosts.useMosh')} description={t('hosts.moshHint')}>
          <Switch
            label={t('hosts.useMosh')}
            checked={config.mosh.enabled ?? false}
            onCheckedChange={(v) => onChange((c) => ({ ...c, mosh: { ...c.mosh, enabled: v } }))}
          />
        </Row>
        {config.mosh.enabled ? (
          <Field label={t('hosts.moshCommand')} className="mt-2">
            {(id) => (
              <Input
                id={id}
                className="font-mono text-[12px]"
                value={config.mosh.serverCommand ?? ''}
                placeholder="mosh-server new -s -l LANG=en_US.UTF-8"
                onChange={(e) =>
                  onChange((c) => ({
                    ...c,
                    mosh: { ...c.mosh, serverCommand: e.target.value || null },
                  }))
                }
              />
            )}
          </Field>
        ) : null}
      </Card>

      <Card title={t('hosts.telnet')}>
        <Row label={t('hosts.enableTelnet')} description={t('hosts.telnetWarning')}>
          <Switch
            label={t('hosts.enableTelnet')}
            checked={config.telnet.enabled ?? false}
            onCheckedChange={(v) =>
              onChange((c) => ({ ...c, telnet: { ...c.telnet, enabled: v } }))
            }
          />
        </Row>
        {config.telnet.enabled ? (
          <Field label={t('hosts.port')} className="mt-2">
            {(id) => (
              <Input
                id={id}
                inputMode="numeric"
                value={config.telnet.port ?? ''}
                placeholder="23"
                onChange={(e) =>
                  onChange((c) => ({
                    ...c,
                    telnet: { ...c.telnet, port: Number(e.target.value) || null },
                  }))
                }
              />
            )}
          </Field>
        ) : null}
      </Card>

      <Card title={t('hosts.appearance')}>
        <div className="grid grid-cols-[1fr_96px] gap-3">
          <Field label={t('hosts.theme')}>
            {(id) => (
              <Select
                id={id}
                value={config.terminal.theme ?? ''}
                onChange={(e) =>
                  onChange((c) => ({
                    ...c,
                    terminal: { ...c.terminal, theme: e.target.value || null },
                  }))
                }
              >
                <option value="">
                  {inheritedHint ? t('hosts.inherited') : t('common.default')}
                </option>
                {TERMINAL_THEMES.map((th) => (
                  <option key={th.id} value={th.id}>
                    {th.name}
                  </option>
                ))}
              </Select>
            )}
          </Field>
          <Field label={t('hosts.fontSize')}>
            {(id) => (
              <Input
                id={id}
                inputMode="numeric"
                value={config.terminal.fontSize ?? ''}
                placeholder="14"
                onChange={(e) =>
                  onChange((c) => ({
                    ...c,
                    terminal: { ...c.terminal, fontSize: Number(e.target.value) || null },
                  }))
                }
              />
            )}
          </Field>
        </div>
      </Card>
    </>
  );
}

export function InspectorShell({
  title,
  children,
  footer,
  onClose,
}: {
  title: ReactNode;
  children: ReactNode;
  footer?: ReactNode;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex h-12 shrink-0 items-center justify-between px-4">
        <h2 className="text-[14px] font-semibold">{title}</h2>
        <Button size="icon-sm" variant="ghost" aria-label={t('common.close')} onClick={onClose}>
          <X className="size-4" />
        </Button>
      </div>
      <div className="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto px-4 pb-4">{children}</div>
      {footer ? <div className="border-border flex gap-2 border-t p-3">{footer}</div> : null}
    </div>
  );
}

export function HostInspector({ host }: { host: Host }) {
  const { t } = useTranslation();
  const bp = useBreakpoint();
  const auto = bp !== 'phone';
  const save = useSaveHost();
  const del = useDeleteHost();
  const hosts = useHosts().data ?? EMPTY;
  const select = useApp((s) => s.select);
  const sessions = useSessions();
  const { draft, update, dirty, flush } = useAutoSave(host, (h) => save.mutate(h), { auto });
  const set = (patch: Partial<Host>) => {
    update((d) => ({ ...d, ...patch }));
  };

  return (
    <InspectorShell
      title={t('hosts.hostDetails')}
      onClose={() => select(null)}
      footer={
        <>
          {!auto ? (
            <Button variant="secondary" className="flex-1" disabled={!dirty} onClick={flush}>
              {t('common.save')}
            </Button>
          ) : null}
          <Button
            variant="primary"
            className="flex-1"
            onClick={() => {
              if (dirty) flush();
              sessions.connectHost(draft);
            }}
          >
            <Play className="size-4" />
            {t('hosts.connect')}
          </Button>
        </>
      }
    >
      <Card title={t('hosts.address')}>
        <div className="flex items-center gap-3">
          <OsIcon os={draft.os} />
          <Input
            aria-label={t('hosts.address')}
            value={draft.address}
            placeholder="192.168.1.10 / host.example.com"
            onChange={(e) => set({ address: e.target.value.trim() })}
          />
        </div>
      </Card>
      <Card title={t('hosts.general')}>
        <div className="grid gap-3">
          <Field label={t('hosts.label')}>
            {(id) => (
              <Input id={id} value={draft.label} onChange={(e) => set({ label: e.target.value })} />
            )}
          </Field>
          <Field label={t('hosts.parentGroup')}>
            {(id) => (
              <GroupSelect id={id} value={draft.group} onChange={(g) => set({ group: g })} />
            )}
          </Field>
          <Field label={t('hosts.tags')}>
            {() => (
              <TagInput
                ariaLabel={t('hosts.tags')}
                value={draft.tags}
                suggestions={allTags(hosts)}
                placeholder={t('hosts.tagsPlaceholder')}
                onChange={(tags) => set({ tags })}
              />
            )}
          </Field>
        </div>
      </Card>
      <ConfigEditor
        selfId={draft.id}
        config={draft.config}
        inheritedHint={Boolean(draft.group)}
        onChange={(fn) => update((d) => ({ ...d, config: fn(d.config) }))}
      />
      <Card title={t('hosts.notes')}>
        <Textarea
          aria-label={t('hosts.notes')}
          value={draft.notes ?? ''}
          onChange={(e) => set({ notes: e.target.value || null })}
        />
      </Card>
      <Button
        variant="ghost"
        className="text-danger self-start"
        onClick={() => {
          del.mutate(draft.id);
          select(null);
        }}
      >
        <Trash2 className="size-4" />
        {t('hosts.deleteHost')}
      </Button>
    </InspectorShell>
  );
}
