// SPDX-License-Identifier: GPL-3.0-or-later
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { ArrowRightLeft, Globe, Laptop, Play, Plus, Server, Square, Trash2 } from 'lucide-react';
import { useState, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';

import { Button } from '@/components/ui/button';
import { Badge, Select, Switch } from '@/components/ui/controls';
import { Field, Input } from '@/components/ui/input';
import { EmptyState } from '@/components/ui/misc';
import { ContextMenu, Dialog } from '@/components/ui/overlay';
import { cn } from '@/lib/cn';
import { EMPTY } from '@/lib/empty';
import { commands, errorMessage, newId, type ForwardKind, type PortForwardRule } from '@/lib/ipc';
import { useDeleteForward, useForwards, useHosts, useSaveForward } from '@/lib/queries';

/** OpenSSH-equivalent flag shown for each rule (mirrors `PortForwardRule::openssh_flag`). */
export function opensshFlag(
  r: Pick<PortForwardRule, 'kind' | 'bindAddress' | 'bindPort' | 'destHost' | 'destPort'>,
): string {
  const wrap = (h: string) => (h.includes(':') ? `[${h}]` : h);
  const bind = `${wrap(r.bindAddress)}:${String(r.bindPort)}`;
  if (r.kind === 'dynamic') return `-D ${bind}`;
  return `${r.kind === 'local' ? '-L' : '-R'} ${bind}:${wrap(r.destHost ?? 'localhost')}:${String(r.destPort ?? 0)}`;
}

function KindCard({
  kind,
  active,
  onClick,
  icon,
  title,
  description,
}: {
  kind: ForwardKind;
  active: boolean;
  onClick: () => void;
  icon: ReactNode;
  title: string;
  description: string;
}) {
  return (
    <button
      type="button"
      role="radio"
      aria-checked={active}
      data-kind={kind}
      onClick={onClick}
      className={cn(
        'bg-surface-2 flex flex-col gap-1.5 rounded-xl p-3 text-left ring-2 ring-transparent',
        active && 'ring-accent',
      )}
    >
      <span className="text-accent-text [&>svg]:size-5">{icon}</span>
      <span className="text-[13px] font-medium">{title}</span>
      <span className="text-subtle text-xs">{description}</span>
    </button>
  );
}

function RuleEditor({ rule, onClose }: { rule: PortForwardRule; onClose: () => void }) {
  const { t } = useTranslation();
  const hosts = useHosts().data ?? EMPTY;
  const save = useSaveForward();
  const [d, setD] = useState(rule);
  const valid =
    d.label.trim() &&
    d.host &&
    d.bindPort > 0 &&
    (d.kind === 'dynamic' || (d.destHost && d.destPort));
  return (
    <Dialog
      open
      onOpenChange={(o) => {
        if (!o) onClose();
      }}
      title={rule.createdAt ? t('forwarding.edit') : t('forwarding.new')}
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            {t('common.cancel')}
          </Button>
          <Button
            variant="primary"
            disabled={!valid}
            onClick={() => save.mutate(d, { onSuccess: onClose })}
          >
            {t('common.save')}
          </Button>
        </>
      }
    >
      <div className="grid gap-4">
        <div
          role="radiogroup"
          aria-label={t('forwarding.type')}
          className="grid grid-cols-1 gap-2 sm:grid-cols-3"
        >
          <KindCard
            kind="local"
            active={d.kind === 'local'}
            onClick={() => setD({ ...d, kind: 'local' })}
            icon={<Laptop />}
            title={t('forwarding.local')}
            description={t('forwarding.localHint')}
          />
          <KindCard
            kind="remote"
            active={d.kind === 'remote'}
            onClick={() => setD({ ...d, kind: 'remote' })}
            icon={<Server />}
            title={t('forwarding.remote')}
            description={t('forwarding.remoteHint')}
          />
          <KindCard
            kind="dynamic"
            active={d.kind === 'dynamic'}
            onClick={() => setD({ ...d, kind: 'dynamic' })}
            icon={<Globe />}
            title={t('forwarding.dynamic')}
            description={t('forwarding.dynamicHint')}
          />
        </div>
        <Field label={t('forwarding.label')}>
          {(id) => (
            <Input
              id={id}
              value={d.label}
              onChange={(e) => setD({ ...d, label: e.target.value })}
            />
          )}
        </Field>
        <Field label={t('forwarding.host')} hint={t('forwarding.hostHint')}>
          {(id) => (
            <Select id={id} value={d.host} onChange={(e) => setD({ ...d, host: e.target.value })}>
              <option value="">{t('forwarding.chooseHost')}</option>
              {hosts.map((h) => (
                <option key={h.id} value={h.id}>
                  {h.label}
                </option>
              ))}
            </Select>
          )}
        </Field>
        <div className="grid grid-cols-[1fr_100px] gap-3">
          <Field
            label={d.kind === 'remote' ? t('forwarding.remoteBind') : t('forwarding.localBind')}
          >
            {(id) => (
              <Input
                id={id}
                value={d.bindAddress}
                onChange={(e) => setD({ ...d, bindAddress: e.target.value })}
              />
            )}
          </Field>
          <Field label={t('hosts.port')}>
            {(id) => (
              <Input
                id={id}
                inputMode="numeric"
                value={d.bindPort || ''}
                onChange={(e) => setD({ ...d, bindPort: Number(e.target.value) || 0 })}
              />
            )}
          </Field>
        </div>
        {d.kind !== 'dynamic' ? (
          <div className="grid grid-cols-[1fr_100px] gap-3">
            <Field label={t('forwarding.destination')}>
              {(id) => (
                <Input
                  id={id}
                  value={d.destHost ?? ''}
                  placeholder="localhost"
                  onChange={(e) => setD({ ...d, destHost: e.target.value || null })}
                />
              )}
            </Field>
            <Field label={t('hosts.port')}>
              {(id) => (
                <Input
                  id={id}
                  inputMode="numeric"
                  value={d.destPort ?? ''}
                  onChange={(e) => setD({ ...d, destPort: Number(e.target.value) || null })}
                />
              )}
            </Field>
          </div>
        ) : null}
        <label className="flex items-center justify-between text-[13px]">
          {t('forwarding.autoStart')}
          <Switch
            checked={d.autoStart}
            onCheckedChange={(v) => setD({ ...d, autoStart: v })}
            label={t('forwarding.autoStart')}
          />
        </label>
        {d.bindAddress !== '127.0.0.1' &&
        d.bindAddress !== 'localhost' &&
        d.bindAddress !== '::1' ? (
          <p className="text-warning text-xs">{t('forwarding.exposedWarning')}</p>
        ) : null}
        <code className="bg-surface-2 text-muted selectable rounded-lg px-3 py-2 font-mono text-[12px]">
          ssh {opensshFlag(d)}
        </code>
      </div>
    </Dialog>
  );
}

export function ForwardingView() {
  const { t } = useTranslation();
  const rules = useForwards().data ?? EMPTY;
  const hosts = useHosts().data ?? EMPTY;
  const del = useDeleteForward();
  const qc = useQueryClient();
  const active =
    useQuery({
      queryKey: ['forwardStatus'],
      queryFn: () => commands.forwardStatus(),
      refetchInterval: 3000,
    }).data ?? EMPTY;
  const [editing, setEditing] = useState<PortForwardRule | null>(null);
  const blank = (): PortForwardRule => ({
    id: newId(),
    label: '',
    kind: 'local',
    host: hosts[0]?.id ?? '',
    bindAddress: '127.0.0.1',
    bindPort: 8080,
    destHost: 'localhost',
    destPort: 80,
    autoStart: false,
    createdAt: 0,
    updatedAt: 0,
  });
  const toggle = async (r: PortForwardRule) => {
    try {
      if (active.some((a) => a.ruleId === r.id)) await commands.forwardStop(r.id);
      else await commands.forwardStart(r.id);
    } catch (e) {
      toast.error(errorMessage(e));
    } finally {
      await qc.invalidateQueries({ queryKey: ['forwardStatus'] });
    }
  };
  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex items-center px-4 pt-5 sm:px-6">
        <p className="text-muted text-[13px]">{t('forwarding.description')}</p>
        <Button variant="primary" size="sm" className="ml-auto" onClick={() => setEditing(blank())}>
          <Plus className="size-4" />
          <span className="font-semibold tracking-wide uppercase">{t('forwarding.new')}</span>
        </Button>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto px-4 py-4 sm:px-6">
        {rules.length === 0 ? (
          <EmptyState
            icon={<ArrowRightLeft />}
            title={t('forwarding.empty')}
            description={t('forwarding.emptyDescription')}
            action={
              <Button variant="primary" onClick={() => setEditing(blank())}>
                {t('forwarding.new')}
              </Button>
            }
          />
        ) : (
          <div className="grid gap-2">
            {rules.map((r) => {
              const status = active.find((a) => a.ruleId === r.id);
              const host = hosts.find((h) => h.id === r.host);
              return (
                <ContextMenu
                  key={r.id}
                  items={[
                    { label: t('common.edit'), onSelect: () => setEditing(r) },
                    {
                      label: t('common.duplicate'),
                      onSelect: () =>
                        setEditing({ ...r, id: newId(), label: `${r.label} (copy)`, createdAt: 0 }),
                    },
                    { separator: true },
                    {
                      label: t('common.delete'),
                      icon: <Trash2 />,
                      danger: true,
                      onSelect: () => del.mutate(r.id),
                    },
                  ]}
                >
                  <div
                    className="bg-surface-2 flex items-center gap-3 rounded-[var(--radius-card)] p-3.5"
                    onDoubleClick={() => void toggle(r)}
                  >
                    <div
                      className={cn(
                        'flex size-9 items-center justify-center rounded-[11px]',
                        status ? 'bg-success-soft text-success' : 'bg-surface-3 text-muted',
                      )}
                    >
                      <ArrowRightLeft className="size-4" />
                    </div>
                    <div className="min-w-0 flex-1">
                      <div className="flex items-center gap-2">
                        <span className="truncate font-medium">{r.label}</span>
                        <Badge>{t(`forwarding.${r.kind}`)}</Badge>
                        {status ? (
                          <Badge tone="success">
                            {t('forwarding.active', { count: status.connections })}
                          </Badge>
                        ) : null}
                      </div>
                      <div className="text-subtle truncate font-mono text-[11.5px]">
                        {opensshFlag(r)} · {host?.label ?? t('hosts.missingHost')}
                      </div>
                    </div>
                    <Button
                      size="sm"
                      variant={status ? 'secondary' : 'primary'}
                      onClick={() => void toggle(r)}
                    >
                      {status ? <Square className="size-3.5" /> : <Play className="size-3.5" />}
                      {status ? t('forwarding.stop') : t('forwarding.start')}
                    </Button>
                  </div>
                </ContextMenu>
              );
            })}
          </div>
        )}
      </div>
      {editing ? <RuleEditor rule={editing} onClose={() => setEditing(null)} /> : null}
    </div>
  );
}
