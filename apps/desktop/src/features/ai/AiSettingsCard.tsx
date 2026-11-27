// SPDX-License-Identifier: GPL-3.0-or-later
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { Brain, Download, HardDrive, Trash2 } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';

import { Button } from '@/components/ui/button';
import { Badge, Switch } from '@/components/ui/controls';
import { Card, Row } from '@/components/ui/misc';
import { commands, errorMessage } from '@/lib/ipc';
import { useSettings } from '@/lib/queries';

import { DEFAULT_AI_SETTINGS, type AiSettings } from './settings';

function formatBytes(n: number): string {
  if (n < 1024) return `${String(n)} B`;
  const units = ['KB', 'MB', 'GB'];
  let v = n / 1024;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v.toFixed(v < 10 ? 1 : 0)} ${units[i] ?? 'GB'}`;
}

export function AiSettingsCard() {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const { settings, update } = useSettings<AiSettings>('ai', DEFAULT_AI_SETTINGS);
  const status = useQuery({
    queryKey: ['aiStatus'],
    queryFn: () => commands.aiStatus(),
    refetchInterval: (q) =>
      q.state.data?.state === 'downloading' || q.state.data?.state === 'loading' ? 500 : false,
  }).data;
  const refresh = () => qc.invalidateQueries({ queryKey: ['aiStatus'] });

  const enable = async (on: boolean) => {
    update({ enabled: on });
    if (on && status?.state === 'notInstalled') {
      try {
        await commands.aiInstall();
        await refresh();
      } catch (e) {
        toast.error(errorMessage(e));
      }
    }
  };

  const total = status?.total ?? 0;
  const pct = status && total > 0 ? Math.round(((status.downloaded ?? 0) / total) * 100) : 0;

  return (
    <>
      <Card title={t('ai.title')}>
        <div className="flex gap-3">
          <div className="bg-accent-soft text-accent flex size-10 shrink-0 items-center justify-center rounded-xl">
            <Brain className="size-5" />
          </div>
          <p className="text-muted text-[13px]">{t('ai.description')}</p>
        </div>
        <Row label={t('ai.enable')} description={t('ai.enableHint')}>
          <Switch
            label={t('ai.enable')}
            checked={settings.enabled}
            onCheckedChange={(v) => void enable(v)}
          />
        </Row>
        <Row label={t('ai.useSelection')}>
          <Switch
            label={t('ai.useSelection')}
            checked={settings.useSelection}
            onCheckedChange={(v) => update({ useSelection: v })}
          />
        </Row>
      </Card>
      <Card title={t('ai.model')}>
        {status ? (
          <div className="grid gap-3">
            <div className="flex items-center gap-2">
              <HardDrive className="text-muted size-4" />
              <span className="text-[13px] font-medium">{status.modelName}</span>
              <Badge
                tone={
                  status.state === 'ready' || status.state === 'installed'
                    ? 'success'
                    : status.state === 'error'
                      ? 'danger'
                      : 'neutral'
                }
              >
                {t(`ai.state.${status.state}`)}
              </Badge>
              <span className="text-subtle ml-auto text-xs">{formatBytes(status.total ?? 0)}</span>
            </div>
            {status.state === 'downloading' ? (
              <div
                className="bg-surface-4 h-1.5 overflow-hidden rounded-full"
                role="progressbar"
                aria-valuenow={pct}
                aria-valuemin={0}
                aria-valuemax={100}
              >
                <div
                  className="bg-accent h-full transition-all"
                  style={{ width: `${String(pct)}%` }}
                />
              </div>
            ) : null}
            {status.error ? <p className="text-danger text-xs">{status.error}</p> : null}
            <p className="text-subtle text-xs">{t('ai.license', { license: status.license })}</p>
            <div className="flex gap-2">
              {status.state === 'notInstalled' || status.state === 'error' ? (
                <Button
                  size="sm"
                  variant="primary"
                  onClick={() =>
                    void commands
                      .aiInstall()
                      .then(refresh)
                      .catch((e: unknown) => toast.error(errorMessage(e)))
                  }
                >
                  <Download className="size-4" />
                  {t('ai.download')}
                </Button>
              ) : null}
              {status.state === 'downloading' ? (
                <Button size="sm" onClick={() => void commands.aiCancel().then(refresh)}>
                  {t('common.cancel')}
                </Button>
              ) : null}
              {status.state === 'installed' || status.state === 'ready' ? (
                <Button
                  size="sm"
                  variant="ghost"
                  className="text-danger"
                  onClick={() => void commands.aiRemove().then(refresh)}
                >
                  <Trash2 className="size-4" />
                  {t('ai.remove')}
                </Button>
              ) : null}
            </div>
          </div>
        ) : null}
      </Card>
      <p className="text-subtle px-1 text-xs">{t('ai.privacy')}</p>
    </>
  );
}
