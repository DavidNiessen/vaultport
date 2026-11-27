// SPDX-License-Identifier: GPL-3.0-or-later
import {
  ChevronDown,
  ChevronRight,
  Copy,
  FolderInput,
  FolderPlus,
  LayoutGrid,
  List,
  Pencil,
  Play,
  Plus,
  Server,
  SquareTerminal,
  Star,
  Tag,
  Trash2,
  Upload,
  Usb,
  Zap,
} from 'lucide-react';
import { useMemo, useRef, useState, type DragEvent, type SubmitEvent } from 'react';
import { useTranslation } from 'react-i18next';

import { GroupIcon, OsIcon } from '@/components/OsIcon';
import { Button } from '@/components/ui/button';
import { Badge, Kbd } from '@/components/ui/controls';
import { EmptyState, SectionHeader } from '@/components/ui/misc';
import { ContextMenu, DropdownMenu, type MenuItemDef } from '@/components/ui/overlay';
import { cn } from '@/lib/cn';
import { EMPTY } from '@/lib/empty';
import type { Group, Host } from '@/lib/ipc';
import { formatShortcut, useBreakpoint, usePlatform } from '@/lib/platform';
import {
  useDeleteGroup,
  useDeleteHost,
  useGroups,
  useHosts,
  useSaveGroup,
  useSaveHost,
} from '@/lib/queries';
import { useApp } from '@/stores/app';

import { useSessions } from '../terminal/sessions';
import { parseQuickConnect } from './quickConnect';
import {
  allTags,
  childGroups,
  groupPath,
  hostCounts,
  hostSubtitle,
  hostsInView,
  newGroup,
  newHost,
} from './model';

const DRAG_HOSTS = 'application/x-ssh-client-hosts';

export function HostsView({ onImport }: { onImport: () => void }) {
  const { t } = useTranslation();
  const platform = usePlatform();
  const bp = useBreakpoint();
  const hosts = useHosts().data ?? EMPTY;
  const groups = useGroups().data ?? EMPTY;
  const {
    currentGroup,
    setCurrentGroup,
    selected,
    select,
    hostView,
    setHostView,
    tagFilter,
    setTagFilter,
  } = useApp();
  const saveHost = useSaveHost();
  const saveGroup = useSaveGroup();
  const deleteHost = useDeleteHost();
  const deleteGroup = useDeleteGroup();
  const sessions = useSessions();
  const [query, setQuery] = useState('');
  const [multi, setMulti] = useState<Set<string>>(new Set());
  const inputRef = useRef<HTMLInputElement>(null);
  const clickTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const quick = useMemo(() => parseQuickConnect(query), [query]);
  const visibleHosts = useMemo(
    () => hostsInView(hosts, groups, currentGroup, quick ? '' : query, tagFilter),
    [hosts, groups, currentGroup, query, quick, tagFilter],
  );
  const subgroups = useMemo(
    () => (query || tagFilter.length ? [] : childGroups(groups, currentGroup)),
    [groups, currentGroup, query, tagFilter],
  );
  const counts = useMemo(() => hostCounts(hosts, groups), [hosts, groups]);
  const tags = useMemo(() => allTags(hosts), [hosts]);
  const path = groupPath(groups, currentGroup);

  const createHost = () => {
    const h = newHost(currentGroup);
    h.label = t('hosts.newHostLabel');
    h.address = 'example.com';
    saveHost.mutate(h, { onSuccess: (saved) => select({ kind: 'host', id: saved.id }) });
  };
  const createGroup = () => {
    const g = newGroup(currentGroup);
    g.label = t('hosts.newGroupLabel');
    saveGroup.mutate(g, { onSuccess: (saved) => select({ kind: 'group', id: saved.id }) });
  };

  const onSubmit = (e: SubmitEvent) => {
    e.preventDefault();
    if (quick) {
      sessions.connectQuick(quick);
      setQuery('');
    } else if (visibleHosts[0]) {
      sessions.connectHost(visibleHosts[0]);
    }
  };

  const moveHosts = (ids: string[], group: string | null) => {
    for (const id of ids) {
      const h = hosts.find((x) => x.id === id);
      if (h && h.group !== group) saveHost.mutate({ ...h, group });
    }
    setMulti(new Set());
  };

  const hostMenu = (h: Host): MenuItemDef[] => [
    { label: t('hosts.connect'), icon: <Play />, onSelect: () => sessions.connectHost(h) },
    { label: t('hosts.openSftp'), icon: <FolderInput />, onSelect: () => sessions.openSftp(h) },
    { separator: true },
    {
      label: t('common.edit'),
      icon: <Pencil />,
      onSelect: () => select({ kind: 'host', id: h.id }),
    },
    {
      label: h.favorite ? t('hosts.unfavorite') : t('hosts.favorite'),
      icon: <Star />,
      onSelect: () => saveHost.mutate({ ...h, favorite: !h.favorite }),
    },
    {
      label: t('common.duplicate'),
      icon: <Copy />,
      onSelect: () =>
        saveHost.mutate({ ...h, id: newHost(null).id, label: `${h.label} (copy)`, createdAt: 0 }),
    },
    { separator: true },
    {
      label: t('common.delete'),
      icon: <Trash2 />,
      danger: true,
      onSelect: () => {
        deleteHost.mutate(h.id);
        if (selected?.id === h.id) select(null);
      },
    },
  ];

  const groupMenu = (g: Group): MenuItemDef[] => [
    { label: t('hosts.openGroup'), icon: <FolderInput />, onSelect: () => setCurrentGroup(g.id) },
    {
      label: t('hosts.quickConnect'),
      icon: <Zap />,
      onSelect: () => {
        hostsInView(hosts, groups, g.id, '', []).forEach((h) => sessions.connectHost(h));
      },
    },
    {
      label: t('common.edit'),
      icon: <Pencil />,
      onSelect: () => select({ kind: 'group', id: g.id }),
    },
    { separator: true },
    {
      label: t('common.delete'),
      icon: <Trash2 />,
      danger: true,
      onSelect: () => {
        deleteGroup.mutate(g.id);
        if (selected?.id === g.id) select(null);
      },
    },
  ];

  const onHostDragStart = (e: DragEvent, h: Host) => {
    const ids = multi.has(h.id) ? [...multi] : [h.id];
    e.dataTransfer.setData(DRAG_HOSTS, JSON.stringify(ids));
    e.dataTransfer.setData('text/plain', h.label);
    e.dataTransfer.effectAllowed = 'move';
  };

  const dropOnGroup = (e: DragEvent, group: string | null) => {
    const raw = e.dataTransfer.getData(DRAG_HOSTS);
    if (!raw) return;
    e.preventDefault();
    moveHosts(JSON.parse(raw) as string[], group);
  };

  const phone = bp === 'phone';

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="px-4 pt-4 sm:px-6 sm:pt-5">
        <form onSubmit={onSubmit} className="relative">
          <input
            ref={inputRef}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder={t('hosts.omnibox')}
            aria-label={t('hosts.omnibox')}
            className="bg-surface-2 text-text placeholder:text-subtle focus:border-accent h-11 w-full rounded-xl border border-transparent pr-28 pl-4 text-[14px] focus:outline-none"
            autoCapitalize="off"
            autoCorrect="off"
            spellCheck={false}
          />
          <Button
            type="submit"
            variant={quick ? 'primary' : 'ghost'}
            size="sm"
            disabled={!quick && visibleHosts.length === 0}
            className="absolute top-1/2 right-1.5 -translate-y-1/2 font-semibold tracking-wide uppercase"
          >
            {t('hosts.connect')}
          </Button>
        </form>
        {quick ? (
          <p className="text-muted mt-2 text-xs">
            {t('hosts.quickConnectHint', {
              target: `${quick.username ? `${quick.username}@` : ''}${quick.host}${quick.port ? `:${String(quick.port)}` : ''}`,
            })}{' '}
            <Kbd>↵</Kbd>
          </p>
        ) : null}

        <div className="mt-3 flex flex-wrap items-center gap-2">
          <div className="flex">
            <Button variant="primary" size="sm" onClick={createHost} className="rounded-r-none">
              <Plus className="size-4" />
              <span className="font-semibold tracking-wide uppercase">{t('hosts.newHost')}</span>
            </Button>
            <DropdownMenu
              align="start"
              trigger={
                <Button
                  variant="primary"
                  size="sm"
                  className="border-accent-fg/20 rounded-l-none border-l px-2"
                  aria-label={t('hosts.moreCreate')}
                >
                  <ChevronDown className="size-4" />
                </Button>
              }
              items={[
                { label: t('hosts.newGroup'), icon: <FolderPlus />, onSelect: createGroup },
                { label: t('hosts.import'), icon: <Upload />, onSelect: onImport },
              ]}
            />
          </div>
          {platform.localTerminal ? (
            <Button
              size="sm"
              onClick={() => sessions.openLocal()}
              title={formatShortcut(platform, 'Mod+L')}
            >
              <SquareTerminal className="size-4" />
              <span className="font-semibold tracking-wide uppercase">{t('hosts.terminal')}</span>
            </Button>
          ) : null}
          {!phone ? (
            <Button size="sm" onClick={() => sessions.openSerialPicker()}>
              <Usb className="size-4" />
              <span className="font-semibold tracking-wide uppercase">{t('hosts.serial')}</span>
            </Button>
          ) : null}
          <div className="ml-auto flex items-center gap-1">
            <DropdownMenu
              trigger={
                <Button
                  size="sm"
                  variant={tagFilter.length ? 'primary' : 'ghost'}
                  aria-label={t('hosts.filterTags')}
                >
                  <Tag className="size-4" />
                  {tagFilter.length ? tagFilter.length : null}
                </Button>
              }
              items={
                tags.length === 0
                  ? [{ label: t('hosts.noTags'), disabled: true }]
                  : [
                      ...tags.map((tag) => ({
                        label: `${tagFilter.includes(tag) ? '✓ ' : ''}${tag}`,
                        onSelect: () =>
                          setTagFilter(
                            tagFilter.includes(tag)
                              ? tagFilter.filter((x) => x !== tag)
                              : [...tagFilter, tag],
                          ),
                      })),
                      { separator: true as const },
                      { label: t('hosts.clearTags'), onSelect: () => setTagFilter([]) },
                    ]
              }
            />
            <Button
              size="icon-sm"
              variant="ghost"
              aria-label={hostView === 'grid' ? t('hosts.listView') : t('hosts.gridView')}
              onClick={() => setHostView(hostView === 'grid' ? 'list' : 'grid')}
            >
              {hostView === 'grid' ? (
                <List className="size-4" />
              ) : (
                <LayoutGrid className="size-4" />
              )}
            </Button>
          </div>
        </div>

        {path.length > 0 ? (
          <nav
            aria-label={t('hosts.breadcrumb')}
            className="text-muted mt-4 flex flex-wrap items-center gap-1 text-[13px]"
          >
            <button
              type="button"
              className="hover:text-text"
              onClick={() => setCurrentGroup(null)}
              onDragOver={(e) => e.preventDefault()}
              onDrop={(e) => dropOnGroup(e, null)}
            >
              {t('hosts.allHosts')}
            </button>
            {path.map((g) => (
              <span key={g.id} className="flex items-center gap-1">
                <ChevronRight className="size-3.5" />
                <button
                  type="button"
                  className={cn(
                    'hover:text-text',
                    g.id === currentGroup && 'text-text font-medium',
                  )}
                  onClick={() => setCurrentGroup(g.id)}
                >
                  {g.label}
                </button>
              </span>
            ))}
          </nav>
        ) : null}
        {tagFilter.length > 0 && (
          <div className="mt-3 flex flex-wrap gap-1.5">
            {tagFilter.map((tag) => (
              <button
                key={tag}
                type="button"
                onClick={() => setTagFilter(tagFilter.filter((x) => x !== tag))}
              >
                <Badge tone="accent">{tag} ×</Badge>
              </button>
            ))}
          </div>
        )}
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto px-4 pt-2 pb-6 sm:px-6">
        {subgroups.length > 0 && (
          <>
            <SectionHeader>{t('hosts.groups')}</SectionHeader>
            <div className="grid grid-cols-1 gap-2.5 sm:grid-cols-2 xl:grid-cols-3 2xl:grid-cols-4">
              {subgroups.map((g) => (
                <ContextMenu key={g.id} items={groupMenu(g)}>
                  <button
                    type="button"
                    onClick={() => select({ kind: 'group', id: g.id })}
                    onDoubleClick={() => setCurrentGroup(g.id)}
                    onDragOver={(e) => e.preventDefault()}
                    onDrop={(e) => dropOnGroup(e, g.id)}
                    className={cn(
                      'bg-surface-2 hover:bg-surface-3 flex h-16 items-center gap-3 rounded-[var(--radius-card)] px-3.5 text-left ring-2 ring-transparent transition',
                      selected?.id === g.id && 'ring-accent',
                    )}
                  >
                    <GroupIcon />
                    <span className="min-w-0 flex-1">
                      <span className="block truncate font-medium">{g.label}</span>
                      <span className="text-subtle block text-xs">
                        {t('hosts.hostCount', { count: counts.get(g.id) ?? 0 })}
                      </span>
                    </span>
                    {phone ? (
                      <ChevronRight
                        className="text-subtle size-4"
                        onClick={(e) => {
                          e.stopPropagation();
                          setCurrentGroup(g.id);
                        }}
                      />
                    ) : null}
                  </button>
                </ContextMenu>
              ))}
            </div>
          </>
        )}

        <SectionHeader
          actions={
            multi.size > 0 ? (
              <span className="text-muted text-xs">
                {t('hosts.selected', { count: multi.size })}{' '}
                <button
                  type="button"
                  className="text-accent ml-2"
                  onClick={() => setMulti(new Set())}
                >
                  {t('common.clear')}
                </button>
              </span>
            ) : null
          }
        >
          {t('hosts.hosts')}
        </SectionHeader>
        {visibleHosts.length === 0 ? (
          <EmptyState
            icon={<Server />}
            title={query || tagFilter.length ? t('hosts.noMatches') : t('hosts.emptyTitle')}
            description={query || tagFilter.length ? undefined : t('hosts.emptyDescription')}
            action={
              query || tagFilter.length ? undefined : (
                <div className="flex gap-2">
                  <Button variant="primary" onClick={createHost}>
                    <Plus className="size-4" />
                    {t('hosts.newHost')}
                  </Button>
                  <Button onClick={onImport}>
                    <Upload className="size-4" />
                    {t('hosts.import')}
                  </Button>
                </div>
              )
            }
          />
        ) : (
          <div
            className={cn(
              hostView === 'grid'
                ? 'grid grid-cols-1 gap-2.5 sm:grid-cols-2 xl:grid-cols-3 2xl:grid-cols-4'
                : 'flex flex-col gap-1',
            )}
          >
            {visibleHosts.map((h) => (
              <ContextMenu key={h.id} items={hostMenu(h)}>
                <div className="relative">
                  <button
                    type="button"
                    draggable
                    onDragStart={(e) => onHostDragStart(e, h)}
                    onClick={(e) => {
                      if (e.metaKey || e.ctrlKey || e.shiftKey) {
                        setMulti((m) => {
                          const n = new Set(m);
                          if (n.has(h.id)) n.delete(h.id);
                          else n.add(h.id);
                          return n;
                        });
                      } else if (phone) {
                        sessions.connectHost(h);
                      } else {
                        // Opening the inspector reflows the grid; wait until a double-click
                        // could no longer happen so the second click hits the same card.
                        if (clickTimer.current) clearTimeout(clickTimer.current);
                        clickTimer.current = setTimeout(() => {
                          select({ kind: 'host', id: h.id });
                        }, 220);
                      }
                    }}
                    onDoubleClick={() => {
                      if (clickTimer.current) clearTimeout(clickTimer.current);
                      sessions.connectHost(h);
                    }}
                    onKeyDown={(e) => {
                      if (e.key === 'Enter') sessions.connectHost(h);
                    }}
                    className={cn(
                      'bg-surface-2 hover:bg-surface-3 group flex w-full items-center gap-3 text-left ring-2 ring-transparent transition',
                      hostView === 'grid'
                        ? 'h-16 rounded-[var(--radius-card)] px-3.5'
                        : 'h-12 rounded-xl px-3',
                      (selected?.id === h.id || multi.has(h.id)) && 'ring-accent',
                    )}
                  >
                    <OsIcon os={h.os} size={hostView === 'grid' ? 'md' : 'sm'} />
                    <span className="min-w-0 flex-1">
                      <span className="flex items-center gap-1.5">
                        <span className="truncate font-medium">{h.label}</span>
                        {h.favorite ? (
                          <Star className="text-warning size-3 shrink-0 fill-current" />
                        ) : null}
                      </span>
                      <span className="text-subtle block truncate text-xs">{hostSubtitle(h)}</span>
                    </span>
                    {phone ? null : (
                      <Play className="text-subtle size-4 opacity-0 transition group-hover:opacity-100" />
                    )}
                  </button>
                  {phone ? (
                    <button
                      type="button"
                      aria-label={t('common.edit')}
                      onClick={() => select({ kind: 'host', id: h.id })}
                      className="text-subtle hover:text-text absolute top-1/2 right-2 -translate-y-1/2 p-2"
                    >
                      <Pencil className="size-4" />
                    </button>
                  ) : null}
                </div>
              </ContextMenu>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
