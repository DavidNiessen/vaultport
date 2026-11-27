// SPDX-License-Identifier: GPL-3.0-or-later
/** Read-only replay of a recorded (decrypted on demand) session log. */
import { useQuery } from '@tanstack/react-query';
import { FitAddon } from '@xterm/addon-fit';
import { SearchAddon } from '@xterm/addon-search';
import { Terminal } from '@xterm/xterm';
import { Pause, Play, Search } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { commands, errorMessage } from '@/lib/ipc';
import { useColorScheme } from '@/lib/platform';
import { useApp, type LogTab } from '@/stores/app';

import { findTheme } from '../terminal/themes';

export function LogViewer({ tab }: { tab: LogTab }) {
  const { t } = useTranslation();
  const ref = useRef<HTMLDivElement>(null);
  const termRef = useRef<Terminal | null>(null);
  const searchRef = useRef<SearchAddon | null>(null);
  const scheme = useColorScheme(useApp((s) => s.theme));
  const [playing, setPlaying] = useState(false);
  const [query, setQuery] = useState('');
  const log = useQuery({
    queryKey: ['log', tab.logId],
    queryFn: () => commands.logRead(tab.logId),
  });

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const term = new Terminal({
      disableStdin: true,
      theme: findTheme(null, scheme).colors,
      fontFamily: "'JetBrains Mono', monospace",
      fontSize: 13,
      scrollback: 100_000,
      convertEol: false,
    });
    const fit = new FitAddon();
    const search = new SearchAddon();
    term.loadAddon(fit);
    term.loadAddon(search);
    term.open(el);
    fit.fit();
    termRef.current = term;
    searchRef.current = search;
    const ro = new ResizeObserver(() => fit.fit());
    ro.observe(el);
    return () => {
      ro.disconnect();
      term.dispose();
    };
  }, [scheme]);

  useEffect(() => {
    const term = termRef.current;
    const data = log.data;
    if (!term || !data) return;
    term.reset();
    if (data.cols && data.rows) term.resize(data.cols, data.rows);
    if (!playing) {
      for (const e of data.events) if (e.kind === 'o') term.write(e.data);
      return;
    }
    let cancelled = false;
    let i = 0;
    const start = performance.now();
    const tick = () => {
      if (cancelled) return;
      const elapsed = (performance.now() - start) / 1000;
      while (i < data.events.length && (data.events[i]?.time ?? 0) <= elapsed) {
        const e = data.events[i];
        if (e?.kind === 'o') term.write(e.data);
        if (e?.kind === 'r') {
          const [c, r] = e.data.split('x').map(Number);
          if (c && r) term.resize(c, r);
        }
        i++;
      }
      if (i < data.events.length) requestAnimationFrame(tick);
      else setPlaying(false);
    };
    requestAnimationFrame(tick);
    return () => {
      cancelled = true;
    };
  }, [log.data, playing]);

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="border-border flex h-11 shrink-0 items-center gap-2 border-b px-3">
        <Button size="sm" variant="ghost" onClick={() => setPlaying(!playing)} disabled={!log.data}>
          {playing ? <Pause className="size-4" /> : <Play className="size-4" />}
          {playing ? t('logs.pause') : t('logs.replay')}
        </Button>
        <span className="text-subtle text-xs">
          {log.data ? t('logs.duration', { seconds: Math.round(log.data.duration ?? 0) }) : null}
        </span>
        <div className="relative ml-auto">
          <Search className="text-subtle absolute top-1/2 left-2 size-3.5 -translate-y-1/2" />
          <Input
            aria-label={t('logs.searchIn')}
            placeholder={t('logs.searchIn')}
            className="h-8 w-56 pl-7"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') searchRef.current?.findNext(query);
            }}
          />
        </div>
      </div>
      {log.isError ? (
        <p className="text-danger p-4 text-[13px]">{errorMessage(log.error)}</p>
      ) : null}
      <div
        ref={ref}
        className="terminal-host min-h-0 flex-1"
        style={{ background: findTheme(null, scheme).colors.background }}
      />
    </div>
  );
}
