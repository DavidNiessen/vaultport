// SPDX-License-Identifier: GPL-3.0-or-later
import { Fingerprint, ShieldAlert, ShieldCheck, Trash2, Upload } from 'lucide-react';
import { useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/controls';
import { Input } from '@/components/ui/input';
import { EmptyState } from '@/components/ui/misc';
import { ContextMenu } from '@/components/ui/overlay';
import { useDeleteKnownHost, useKnownHosts } from '@/lib/queries';
import { EMPTY } from '@/lib/empty';

export function KnownHostsView({ onImport }: { onImport: () => void }) {
  const { t } = useTranslation();
  const entries = useKnownHosts().data ?? EMPTY;
  const del = useDeleteKnownHost();
  const [query, setQuery] = useState('');
  const filtered = useMemo(
    () =>
      entries
        .filter(
          (e) =>
            !query ||
            e.pattern.toLowerCase().includes(query.toLowerCase()) ||
            e.fingerprint.includes(query),
        )
        .sort((a, b) => a.pattern.localeCompare(b.pattern)),
    [entries, query],
  );
  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex flex-wrap items-center gap-2 px-4 pt-5 sm:px-6">
        <Input
          className="max-w-xs"
          aria-label={t('common.search')}
          placeholder={t('knownHosts.search')}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
        <Button size="sm" className="ml-auto" onClick={onImport}>
          <Upload className="size-4" />
          {t('knownHosts.import')}
        </Button>
      </div>
      <p className="text-muted px-4 pt-3 text-[13px] sm:px-6">{t('knownHosts.description')}</p>
      <div className="min-h-0 flex-1 overflow-y-auto px-4 py-4 sm:px-6">
        {filtered.length === 0 ? (
          <EmptyState
            icon={<Fingerprint />}
            title={t('knownHosts.empty')}
            description={t('knownHosts.emptyDescription')}
          />
        ) : (
          <div className="grid gap-1.5">
            {filtered.map((e) => (
              <ContextMenu
                key={e.id}
                items={[
                  {
                    label: t('common.delete'),
                    icon: <Trash2 />,
                    danger: true,
                    onSelect: () => del.mutate(e.id),
                  },
                ]}
              >
                <div className="bg-surface-2 flex items-center gap-3 rounded-xl px-3.5 py-2.5">
                  {e.marker === 'revoked' ? (
                    <ShieldAlert className="text-danger size-4 shrink-0" />
                  ) : (
                    <ShieldCheck className="text-success size-4 shrink-0" />
                  )}
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-2">
                      <span className="selectable truncate font-mono text-[13px]">{e.pattern}</span>
                      {e.marker === 'certAuthority' ? (
                        <Badge tone="accent">{t('knownHosts.ca')}</Badge>
                      ) : null}
                      {e.marker === 'revoked' ? (
                        <Badge tone="danger">{t('knownHosts.revoked')}</Badge>
                      ) : null}
                    </div>
                    <div className="text-subtle selectable truncate font-mono text-[11px]">
                      {e.keyType} · {e.fingerprint}
                    </div>
                  </div>
                  <Button
                    size="icon-sm"
                    variant="ghost"
                    aria-label={t('common.delete')}
                    onClick={() => del.mutate(e.id)}
                  >
                    <Trash2 className="size-4" />
                  </Button>
                </div>
              </ContextMenu>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
