// SPDX-License-Identifier: GPL-3.0-or-later
/** New-tab launcher: search hosts, reopen recent ones, start workspaces. */
import { LayoutGrid, Search, SquareTerminal } from 'lucide-react';
import { useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { OsIcon } from '@/components/OsIcon';
import { Button } from '@/components/ui/button';
import { Dialog } from '@/components/ui/overlay';
import { newId } from '@/lib/ipc';
import { EMPTY } from '@/lib/empty';
import { usePlatform } from '@/lib/platform';
import { useHosts, useWorkspaces } from '@/lib/queries';
import { useApp } from '@/stores/app';

import { parseQuickConnect } from '../hosts/quickConnect';
import { protocolFor, useSessions } from '../terminal/sessions';

export function NewTabDialog() {
  const { t } = useTranslation();
  const platform = usePlatform();
  const { newTabOpen, setNewTabOpen, openTab } = useApp();
  const hosts = useHosts().data ?? EMPTY;
  const workspaces = useWorkspaces().data ?? EMPTY;
  const sessions = useSessions();
  const [query, setQuery] = useState('');
  const [picked, setPicked] = useState<string[]>([]);
  const recent = useMemo(
    () =>
      hosts
        .filter(
          (h) =>
            !query ||
            `${h.label} ${h.address} ${h.tags.join(' ')}`
              .toLowerCase()
              .includes(query.toLowerCase()),
        )
        .sort((a, b) => (b.lastConnectedAt ?? 0) - (a.lastConnectedAt ?? 0))
        .slice(0, 12),
    [hosts, query],
  );
  const quick = parseQuickConnect(query);
  const close = () => {
    setNewTabOpen(false);
    setQuery('');
    setPicked([]);
  };

  const createWorkspace = () => {
    const wsId = newId();
    const paneIds = picked.map(() => newId());
    picked.forEach((hid, i) => {
      const h = hosts.find((x) => x.id === hid);
      if (!h) return;
      useApp.setState((s) => ({
        tabs: [
          ...s.tabs,
          {
            id: paneIds[i] ?? newId(),
            kind: 'terminal',
            title: h.label,
            hostId: h.id,
            protocol: protocolFor(h),
            sessionId: null,
            status: 'connecting',
            unread: false,
            workspace: wsId,
          },
        ],
      }));
    });
    openTab({
      id: wsId,
      kind: 'workspace',
      title: t('workspaces.defaultName', { count: picked.length }),
      panes: paneIds,
      mode: 'split',
      broadcast: false,
      focused: paneIds[0] ?? null,
      templateId: null,
    });
    close();
  };

  return (
    <Dialog
      open={newTabOpen}
      onOpenChange={(o) => (o ? setNewTabOpen(true) : close())}
      title={t('tabs.new')}
      className="sm:w-[min(92vw,600px)]"
    >
      <div className="relative mb-4">
        <Search className="text-subtle absolute top-1/2 left-3 size-4 -translate-y-1/2" />
        <input
          autoFocus
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') {
              if (quick) {
                sessions.connectQuick(quick);
                close();
              } else if (recent[0]) {
                sessions.connectHost(recent[0]);
                close();
              }
            }
          }}
          placeholder={t('tabs.searchPlaceholder')}
          aria-label={t('tabs.searchPlaceholder')}
          className="bg-surface-3 focus:border-accent h-11 w-full rounded-xl border border-transparent pr-3 pl-9 text-sm outline-none"
        />
      </div>
      {platform.localTerminal ? (
        <Button
          variant="ghost"
          className="mb-3 w-full justify-start"
          onClick={() => {
            sessions.openLocal();
            close();
          }}
        >
          <SquareTerminal className="size-4" />
          {t('hosts.terminal')}
        </Button>
      ) : null}
      {workspaces.length > 0 && !query ? (
        <div className="mb-4">
          <h3 className="text-subtle mb-1.5 text-[11px] font-semibold tracking-wide uppercase">
            {t('workspaces.templates')}
          </h3>
          <div className="grid gap-1">
            {workspaces.map((w) => (
              <button
                key={w.id}
                type="button"
                className="hover:bg-surface-2 flex h-10 items-center gap-3 rounded-lg px-2 text-left text-[13px]"
                onClick={() => {
                  window.dispatchEvent(
                    new CustomEvent('vaultport:restore-workspace', { detail: w.id }),
                  );
                  close();
                }}
              >
                <LayoutGrid className="text-muted size-4" />
                <span className="flex-1">{w.label}</span>
                <span className="text-subtle text-xs">
                  {t('workspaces.connections', { count: w.panes.length })}
                </span>
              </button>
            ))}
          </div>
        </div>
      ) : null}
      <div className="mb-1.5 flex items-center justify-between">
        <h3 className="text-subtle text-[11px] font-semibold tracking-wide uppercase">
          {t('tabs.recent')}
        </h3>
        {picked.length >= 2 ? (
          <Button size="sm" variant="primary" onClick={createWorkspace}>
            <LayoutGrid className="size-4" />
            {t('workspaces.create', { count: picked.length })}
          </Button>
        ) : null}
      </div>
      <div className="grid gap-0.5">
        {recent.map((h) => (
          <div
            key={h.id}
            className="hover:bg-surface-2 flex h-11 items-center gap-3 rounded-lg px-2"
          >
            <input
              type="checkbox"
              aria-label={t('workspaces.select', { host: h.label })}
              className="accent-[var(--accent)]"
              checked={picked.includes(h.id)}
              onChange={(e) =>
                setPicked(e.target.checked ? [...picked, h.id] : picked.filter((x) => x !== h.id))
              }
            />
            <button
              type="button"
              className="flex min-w-0 flex-1 items-center gap-3 text-left"
              onClick={() => {
                sessions.connectHost(h);
                close();
              }}
            >
              <OsIcon os={h.os} size="sm" />
              <span className="min-w-0 flex-1 truncate text-[13px]">{h.label}</span>
              <span className="text-subtle truncate text-xs">{h.address}</span>
            </button>
          </div>
        ))}
      </div>
    </Dialog>
  );
}
