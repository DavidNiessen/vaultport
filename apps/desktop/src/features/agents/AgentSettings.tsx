// SPDX-License-Identifier: GPL-3.0-or-later
/** Settings → AI agents: MCP access for external AI tools, client setup and activity log. */
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { listen } from '@tauri-apps/api/event';
import { writeText } from '@tauri-apps/plugin-clipboard-manager';
import { Bot, Copy, KeyRound, Unplug } from 'lucide-react';
import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';

import { Button } from '@/components/ui/button';
import { Badge, Segmented, Switch } from '@/components/ui/controls';
import { Card, Row } from '@/components/ui/misc';
import {
  commands,
  errorMessage,
  isTauri,
  type AgentSettings as Settings,
  type AgentStatus,
  type AuditEntry,
} from '@/lib/ipc';
import { useHosts } from '@/lib/queries';

import { AUDIT_EVENT } from './events';

const CLIENT_LABELS: Record<string, string> = {
  'claude-code': 'Claude Code',
  'claude-desktop': 'Claude Desktop',
  cursor: 'Cursor',
  vscode: 'VS Code',
  codex: 'Codex',
  generic: 'Other',
};

async function copy(text: string) {
  if (isTauri()) await writeText(text);
  else await navigator.clipboard.writeText(text);
}

export function AgentSettingsCard() {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const status = useQuery({ queryKey: ['agentStatus'], queryFn: () => commands.agentStatus() });
  const audit = useQuery({ queryKey: ['agentAudit'], queryFn: () => commands.agentAuditLog() });

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    void listen(AUDIT_EVENT, () => {
      void qc.invalidateQueries({ queryKey: ['agentAudit'] });
      void qc.invalidateQueries({ queryKey: ['agentStatus'] });
    }).then((u) => (cancelled ? u() : (unlisten = u)));
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [qc]);

  const s = status.data;
  if (!s) return null;
  if (!s.supported) {
    return (
      <Card title={t('agents.title')}>
        <p className="text-muted text-[13px]">{t('agents.desktopOnly')}</p>
      </Card>
    );
  }

  const save = async (patch: Partial<Settings>) => {
    try {
      const next = await commands.agentSaveSettings({ ...s.settings, ...patch });
      qc.setQueryData(['agentStatus'], next);
    } catch (e) {
      toast.error(errorMessage(e));
    }
  };

  return (
    <>
      <Card title={t('agents.title')}>
        <div className="flex gap-3">
          <div className="bg-accent-soft text-accent flex size-10 shrink-0 items-center justify-center rounded-xl">
            <Bot className="size-5" />
          </div>
          <p className="text-muted text-[13px]">{t('agents.description')}</p>
        </div>
        <Row
          label={t('agents.enable')}
          description={
            s.error
              ? t('agents.error', { error: s.error })
              : s.settings.enabled
                ? t('agents.listening')
                : t('agents.enableHint')
          }
        >
          <Switch
            label={t('agents.enable')}
            checked={s.settings.enabled}
            onCheckedChange={(v) => void save({ enabled: v })}
          />
        </Row>
        <Row label={t('agents.approval.policy')} description={t('agents.approval.policyHint')}>
          <Segmented
            ariaLabel={t('agents.approval.policy')}
            value={s.settings.approval}
            onChange={(v) => void save({ approval: v })}
            options={[
              { value: 'always', label: t('agents.approval.always') },
              { value: 'changesOnly', label: t('agents.approval.changesOnly') },
            ]}
          />
        </Row>
        <Row label={t('agents.readOnly')} description={t('agents.readOnlyHint')}>
          <Switch
            label={t('agents.readOnly')}
            checked={s.settings.readOnly}
            onCheckedChange={(v) => void save({ readOnly: v })}
          />
        </Row>
        <HostScope status={s} onSave={save} />
      </Card>
      <ClientSetup status={s} />
      <Card
        title={t('agents.connected')}
        actions={
          <div className="flex gap-1.5">
            <Button
              size="sm"
              variant="ghost"
              disabled={s.clients.length === 0}
              onClick={() => {
                void commands
                  .agentDisconnectAll()
                  .then(() => qc.invalidateQueries({ queryKey: ['agentStatus'] }));
              }}
            >
              <Unplug className="size-4" />
              {t('agents.disconnectAll')}
            </Button>
            <Button
              size="sm"
              variant="ghost"
              onClick={() => {
                commands
                  .agentRotateToken()
                  .then((next) => {
                    qc.setQueryData(['agentStatus'], next);
                    toast.success(t('agents.tokenRotated'));
                  })
                  .catch((e: unknown) => toast.error(errorMessage(e)));
              }}
            >
              <KeyRound className="size-4" />
              {t('agents.rotateToken')}
            </Button>
          </div>
        }
      >
        {s.clients.length === 0 ? (
          <p className="text-subtle text-[13px]">{t('agents.noClients')}</p>
        ) : (
          <ul className="grid gap-1">
            {s.clients.map((c) => (
              <li key={c.id} className="flex items-center justify-between text-[13px]">
                <span>{c.name}</span>
                <span className="text-subtle text-xs">
                  {new Date(c.connectedAt).toLocaleTimeString()}
                </span>
              </li>
            ))}
          </ul>
        )}
      </Card>
      <ActivityLog entries={audit.data ?? []} />
    </>
  );
}

function HostScope({
  status,
  onSave,
}: {
  status: AgentStatus;
  onSave: (p: Partial<Settings>) => Promise<void>;
}) {
  const { t } = useTranslation();
  const hosts = useHosts().data ?? [];
  const selected = new Set(status.settings.hosts);
  return (
    <>
      <Row label={t('agents.hosts')} description={t('agents.hostsHint')}>
        <Segmented
          ariaLabel={t('agents.hosts')}
          value={status.settings.allHosts ? 'all' : 'selected'}
          onChange={(v) => void onSave({ allHosts: v === 'all' })}
          options={[
            { value: 'all', label: t('agents.allHosts') },
            { value: 'selected', label: t('agents.selectedHosts') },
          ]}
        />
      </Row>
      {!status.settings.allHosts ? (
        <ul className="border-border mt-1 grid max-h-56 gap-0.5 overflow-y-auto rounded-lg border p-1.5">
          {hosts.length === 0 ? (
            <li className="text-subtle p-1.5 text-[13px]">{t('agents.noHosts')}</li>
          ) : (
            hosts.map((h) => (
              <li key={h.id}>
                <label className="hover:bg-surface-3 flex cursor-pointer items-center gap-2.5 rounded-md px-2 py-1.5 text-[13px]">
                  <input
                    type="checkbox"
                    className="accent-[var(--color-accent)]"
                    checked={selected.has(h.id)}
                    onChange={(e) => {
                      const next = new Set(selected);
                      if (e.target.checked) next.add(h.id);
                      else next.delete(h.id);
                      void onSave({ hosts: [...next] });
                    }}
                  />
                  <span className="truncate">{h.label || h.address}</span>
                  <span className="text-subtle ml-auto truncate text-xs">{h.address}</span>
                </label>
              </li>
            ))
          )}
        </ul>
      ) : null}
    </>
  );
}

function ClientSetup({ status }: { status: AgentStatus }) {
  const { t } = useTranslation();
  const [client, setClient] = useState('claude-code');
  const config = status.configs.find((c) => c.client === client) ?? status.configs[0];
  if (!config) return null;
  return (
    <Card title={t('agents.setup')}>
      <p className="text-muted mb-3 text-[13px]">{t('agents.setupHint')}</p>
      <div className="mb-3 overflow-x-auto">
        <Segmented
          ariaLabel={t('agents.setup')}
          value={config.client}
          onChange={setClient}
          options={status.configs.map((c) => ({
            value: c.client,
            label: CLIENT_LABELS[c.client] ?? c.client,
          }))}
        />
      </div>
      {config.location ? (
        <p className="text-subtle mb-1.5 text-xs">{t('agents.addTo', { file: config.location })}</p>
      ) : (
        <p className="text-subtle mb-1.5 text-xs">{t('agents.runInTerminal')}</p>
      )}
      <div className="relative">
        <pre
          className="bg-surface-1 overflow-x-auto rounded-lg p-3 pr-11 font-mono text-xs whitespace-pre select-text"
          data-testid="agent-config"
        >
          {config.text}
        </pre>
        <Button
          size="icon"
          variant="ghost"
          className="absolute top-1.5 right-1.5"
          aria-label={t('agents.copy')}
          onClick={() => {
            copy(config.text)
              .then(() => toast.success(t('agents.copied')))
              .catch((e: unknown) => toast.error(errorMessage(e)));
          }}
        >
          <Copy className="size-4" />
        </Button>
      </div>
    </Card>
  );
}

function ActivityLog({ entries }: { entries: AuditEntry[] }) {
  const { t } = useTranslation();
  const tone = { ok: 'success', error: 'danger', denied: 'warning' } as const;
  return (
    <Card title={t('agents.activity')}>
      {entries.length === 0 ? (
        <p className="text-subtle text-[13px]">{t('agents.noActivity')}</p>
      ) : (
        <ul className="divide-border grid max-h-96 divide-y overflow-y-auto">
          {entries.map((e) => (
            <li key={e.id} className="grid gap-0.5 py-2 text-[13px]">
              <div className="flex items-center gap-2">
                <span className="font-medium">{e.tool}</span>
                {e.host ? <span className="text-muted truncate">· {e.host}</span> : null}
                <Badge tone={tone[e.outcome]} className="ml-auto">
                  {t(`agents.outcome.${e.outcome}`)}
                </Badge>
              </div>
              {e.summary ? (
                <code className="text-muted truncate font-mono text-xs">{e.summary}</code>
              ) : null}
              <div className="text-subtle flex gap-2 text-xs">
                <span>{new Date(e.at).toLocaleString()}</span>
                <span>{e.client}</span>
                {e.message ? <span className="text-danger truncate">{e.message}</span> : null}
              </div>
            </li>
          ))}
        </ul>
      )}
    </Card>
  );
}
