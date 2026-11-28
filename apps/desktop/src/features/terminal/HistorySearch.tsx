// SPDX-License-Identifier: GPL-3.0-or-later
/**
 * Command history search (Ctrl+R): a fuzzy, frecency-ranked list of the commands run on this
 * host (and in this session), shown over the bottom of the terminal. Enter puts the command on
 * the prompt for editing; Mod+Enter runs it.
 */
import { useQuery } from '@tanstack/react-query';
import { History, Search } from 'lucide-react';
import { useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { Kbd } from '@/components/ui/controls';
import { cn } from '@/lib/cn';
import { commands } from '@/lib/ipc';
import { formatShortcut, usePlatform } from '@/lib/platform';

import { highlightRuns, matchIndexes } from './autocomplete/match';
import { mergeHistory, rankHistory } from './historyRank';

const MAX_SHOWN = 50;

export function HistorySearch({
  hostId,
  sessionCommands,
  initialQuery,
  onInsert,
  onRun,
  onClose,
}: {
  hostId: string | null;
  sessionCommands: string[];
  initialQuery: string;
  onInsert: (command: string) => void;
  onRun: (command: string) => void;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const platform = usePlatform();
  const [query, setQuery] = useState(initialQuery);
  const [selected, setSelected] = useState(0);
  const listRef = useRef<HTMLDivElement>(null);
  const { data: stored = [], isLoading } = useQuery({
    queryKey: ['history', hostId, 'search'],
    queryFn: () => commands.historyList(hostId, 2000),
    staleTime: 10_000,
  });
  const all = useMemo(() => mergeHistory(sessionCommands, stored), [sessionCommands, stored]);
  const results = useMemo(() => rankHistory(all, query).slice(0, MAX_SHOWN), [all, query]);
  const index = Math.min(selected, Math.max(0, results.length - 1));
  const current = results[index];

  useEffect(() => {
    listRef.current
      ?.querySelector<HTMLElement>(`[data-index="${String(index)}"]`)
      ?.scrollIntoView({ block: 'nearest' });
  }, [index]);

  return (
    <div
      role="dialog"
      aria-label={t('terminal.historySearch.title')}
      data-testid="history-search"
      className="bg-surface-2/95 border-border animate-fade-in absolute inset-x-3 bottom-3 z-30 flex max-h-[60%] flex-col overflow-hidden rounded-xl border shadow-[var(--shadow)] backdrop-blur-md"
    >
      <div className="border-border flex items-center gap-2 border-b px-3">
        <Search className="text-subtle size-4 shrink-0" aria-hidden />
        <input
          autoFocus
          aria-label={t('terminal.historySearch.placeholder')}
          placeholder={t('terminal.historySearch.placeholder')}
          value={query}
          spellCheck={false}
          autoCapitalize="off"
          autoCorrect="off"
          onChange={(e) => {
            setQuery(e.target.value);
            setSelected(0);
          }}
          onKeyDown={(e) => {
            const mod = e.metaKey || e.ctrlKey;
            if (e.key === 'ArrowDown' || (e.ctrlKey && e.key.toLowerCase() === 'r')) {
              e.preventDefault();
              setSelected((i) => Math.min(results.length - 1, i + 1));
            } else if (e.key === 'ArrowUp' || (e.ctrlKey && e.key.toLowerCase() === 's')) {
              e.preventDefault();
              setSelected((i) => Math.max(0, i - 1));
            } else if (e.key === 'Enter' && current) {
              e.preventDefault();
              if (mod || e.shiftKey) onRun(current.command);
              else onInsert(current.command);
            } else if (e.key === 'Escape' || (e.ctrlKey && e.key.toLowerCase() === 'g')) {
              e.preventDefault();
              onClose();
            }
          }}
          onBlur={onClose}
          className="text-text placeholder:text-subtle h-11 min-w-0 flex-1 bg-transparent font-mono text-[13px] outline-none"
        />
        <span className="text-subtle shrink-0 text-[11px]">
          {isLoading ? '…' : t('terminal.historySearch.count', { count: results.length })}
        </span>
      </div>
      <div
        ref={listRef}
        role="listbox"
        aria-label={t('terminal.historySearch.title')}
        className="min-h-0 flex-1 overflow-y-auto py-1"
      >
        {results.length === 0 ? (
          <div className="text-subtle px-3 py-6 text-center text-[13px]">
            {t('terminal.noHistory')}
          </div>
        ) : (
          results.map((r, i) => (
            <div
              key={r.command}
              data-index={i}
              role="option"
              tabIndex={-1}
              aria-selected={i === index}
              onMouseDown={(e) => e.preventDefault()}
              onMouseMove={() => setSelected(i)}
              onClick={() => onInsert(r.command)}
              onKeyDown={() => undefined}
              className={cn(
                'flex h-8 cursor-pointer items-center gap-2 px-3 text-[13px]',
                i === index ? 'bg-accent-soft text-text' : 'text-muted',
              )}
            >
              <History
                className={cn('size-3.5 shrink-0', r.session ? 'text-accent-text' : 'text-subtle')}
                aria-hidden
              />
              <span className="min-w-0 flex-1 truncate font-mono">
                {highlightRuns(r.command, matchIndexes(r.command, query.trim())).map((run, j) =>
                  run.hit ? (
                    <mark key={j} className="text-accent-text bg-transparent font-bold">
                      {run.text}
                    </mark>
                  ) : (
                    <span key={j}>{run.text}</span>
                  ),
                )}
              </span>
              {r.count > 1 ? (
                <span className="text-subtle shrink-0 text-[11px]">×{r.count}</span>
              ) : null}
              {r.session ? (
                <span className="text-subtle bg-surface-3 shrink-0 rounded px-1.5 py-px text-[11px]">
                  {t('terminal.historySearch.thisSession')}
                </span>
              ) : null}
            </div>
          ))
        )}
      </div>
      <div className="border-border text-subtle flex flex-wrap items-center gap-x-3 gap-y-1 border-t px-3 py-1.5 text-[11px]">
        <span className="flex items-center gap-1">
          <Kbd>↵</Kbd> {t('terminal.insert')}
        </span>
        <span className="flex items-center gap-1">
          <Kbd>{formatShortcut(platform, 'Mod+↵')}</Kbd> {t('terminal.run')}
        </span>
        <span className="flex items-center gap-1">
          <Kbd>Esc</Kbd> {t('common.close')}
        </span>
      </div>
    </div>
  );
}
