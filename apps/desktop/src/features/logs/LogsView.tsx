// SPDX-License-Identifier: GPL-3.0-or-later
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { Bookmark, BookmarkCheck, Download, History, Trash2 } from 'lucide-react';
import { useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';

import { OsIcon } from '@/components/OsIcon';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { EmptyState } from '@/components/ui/misc';
import { ContextMenu } from '@/components/ui/overlay';
import { cn } from '@/lib/cn';
import { EMPTY } from '@/lib/empty';
import { commands, errorMessage, newId } from '@/lib/ipc';
import { useHosts } from '@/lib/queries';
import { useApp } from '@/stores/app';

function formatRange(start: number, end: number | null): string {
  const s = new Date(start);
  const time = (d: Date) => d.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' });
  if (!end) return `${s.toLocaleDateString()} ${time(s)}`;
  const e = new Date(end);
  const days = Math.floor(
    (e.setHours(0, 0, 0, 0) - new Date(start).setHours(0, 0, 0, 0)) / 86_400_000,
  );
  return `${s.toLocaleDateString()} ${time(s)} – ${time(new Date(end))}${days > 0 ? ` (+${String(days)}d)` : ''}`;
}

export function LogsView() {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const hosts = useHosts().data ?? EMPTY;
  const logs = useQuery({ queryKey: ['logs'], queryFn: () => commands.listLogs() }).data ?? EMPTY;
  const openTab = useApp((s) => s.openTab);
  const [query, setQuery] = useState('');
  const [savedOnly, setSavedOnly] = useState(false);
  const filtered = useMemo(
    () =>
      logs
        .filter((l) => (savedOnly ? l.bookmarked : true))
        .filter(
          (l) =>
            !query ||
            l.hostLabel.toLowerCase().includes(query.toLowerCase()) ||
            (l.comment ?? '').toLowerCase().includes(query.toLowerCase()),
        )
        .sort((a, b) => (b.startedAt ?? 0) - (a.startedAt ?? 0)),
    [logs, query, savedOnly],
  );
  const refresh = () => qc.invalidateQueries({ queryKey: ['logs'] });
  const bookmark = async (id: string, bookmarked: boolean) => {
    const comment = bookmarked ? window.prompt(t('logs.commentPrompt')) : null;
    await commands
      .logBookmark(id, bookmarked, comment)
      .catch((e: unknown) => toast.error(errorMessage(e)));
    await refresh();
  };
  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex flex-wrap items-center gap-2 px-4 pt-5 sm:px-6">
        <Input
          className="max-w-xs"
          aria-label={t('common.search')}
          placeholder={t('logs.search')}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
        <Button
          size="sm"
          variant={savedOnly ? 'primary' : 'ghost'}
          onClick={() => setSavedOnly(!savedOnly)}
        >
          <Bookmark className="size-4" />
          {t('logs.savedOnly')}
        </Button>
      </div>
      <p className="text-muted px-4 pt-3 text-[13px] sm:px-6">{t('logs.description')}</p>
      <div className="min-h-0 flex-1 overflow-y-auto px-4 py-4 sm:px-6">
        {filtered.length === 0 ? (
          <EmptyState
            icon={<History />}
            title={t('logs.empty')}
            description={t('logs.emptyDescription')}
          />
        ) : (
          <div className="grid gap-1.5">
            {filtered.map((l) => {
              const host = hosts.find((h) => h.id === l.hostId);
              return (
                <ContextMenu
                  key={l.id}
                  items={[
                    {
                      label: t('logs.open'),
                      onSelect: () =>
                        openTab({ id: newId(), kind: 'log', title: l.hostLabel, logId: l.id }),
                    },
                    {
                      label: t('logs.export'),
                      icon: <Download />,
                      onSelect: () =>
                        void commands
                          .logExport(l.id)
                          .then(() => toast.success(t('logs.exported')))
                          .catch((e: unknown) => toast.error(errorMessage(e))),
                    },
                    { separator: true },
                    {
                      label: t('common.delete'),
                      icon: <Trash2 />,
                      danger: true,
                      disabled: l.bookmarked,
                      onSelect: () => void commands.logDelete(l.id).then(refresh),
                    },
                  ]}
                >
                  <button
                    type="button"
                    onClick={() =>
                      openTab({ id: newId(), kind: 'log', title: l.hostLabel, logId: l.id })
                    }
                    className="bg-surface-2 hover:bg-surface-3 grid grid-cols-[auto_1fr_auto] items-center gap-3 rounded-xl px-3.5 py-2.5 text-left sm:grid-cols-[auto_1.4fr_1fr_auto]"
                  >
                    <OsIcon os={host?.os} size="sm" />
                    <div className="min-w-0">
                      <div className="truncate text-[13px] font-medium">{l.hostLabel}</div>
                      <div className="text-subtle truncate text-xs">
                        {l.comment ?? formatRange(l.startedAt ?? 0, l.endedAt)}
                      </div>
                    </div>
                    <div className="text-subtle hidden truncate text-xs sm:block">
                      {formatRange(l.startedAt ?? 0, l.endedAt)} · {l.deviceName}
                    </div>
                    <span
                      role="button"
                      tabIndex={0}
                      aria-label={l.bookmarked ? t('logs.unbookmark') : t('logs.bookmark')}
                      onClick={(e) => {
                        e.stopPropagation();
                        void bookmark(l.id, !l.bookmarked);
                      }}
                      onKeyDown={(e) => {
                        if (e.key === 'Enter') void bookmark(l.id, !l.bookmarked);
                      }}
                      className={cn(
                        'rounded-md p-1.5',
                        l.bookmarked ? 'text-accent' : 'text-subtle hover:text-text',
                      )}
                    >
                      {l.bookmarked ? (
                        <BookmarkCheck className="size-4" />
                      ) : (
                        <Bookmark className="size-4" />
                      )}
                    </span>
                  </button>
                </ContextMenu>
              );
            })}
          </div>
        )}
      </div>
    </div>
  );
}
