// SPDX-License-Identifier: GPL-3.0-or-later
import {
  ArrowRight,
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
  Search,
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
import { Badge, Kbd, Segmented } from '@/components/ui/controls';
import { EmptyState, PageHeader, SectionHeader } from '@/components/ui/misc';
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
  hostProtocol,
  hostTarget,
  hostsInView,
  newGroup,
  newHost,
} from './model';

const DRAG_HOSTS = 'application/x-vaultport-hosts';

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
  const filtering = Boolean(query) || tagFilter.length > 0;
  const quickTarget = quick
    ? `${quick.username ? `${quick.username}@` : ''}${quick.host}${quick.port ? `:${String(quick.port)}` : ''}`
    : '';

  const toggleMulti = (id: string) =>
    setMulti((m) => {
      const n = new Set(m);
      if (n.has(id)) n.delete(id);
      else n.add(id);
      return n;
    });

  const hostCard = (h: Host) => {
    const isSelected = selected?.id === h.id || multi.has(h.id);
    const protocol = hostProtocol(h);
    const grid = hostView === 'grid';
    return (
      <ContextMenu key={h.id} items={hostMenu(h)}>
        <div className="group/card relative">
          <button
            type="button"
            draggable
            onDragStart={(e) => onHostDragStart(e, h)}
            onClick={(e) => {
              if (e.metaKey || e.ctrlKey || e.shiftKey) {
                toggleMulti(h.id);
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
              'bg-surface-2 border-border flex w-full items-center gap-3 border text-left transition-[background-color,border-color,box-shadow] duration-150',
              // Hover: a soft wash of the preset colour with an accent edge.
              'hover:border-border-strong hover:bg-[linear-gradient(90deg,var(--accent-soft),transparent_85%)] hover:shadow-[inset_3px_0_0_var(--accent)]',
              grid
                ? 'rounded-[var(--radius-card)] p-3.5 pr-12'
                : 'h-14 rounded-[14px] px-3.5 pr-12',
              isSelected && 'border-accent! ring-accent-soft ring-3',
            )}
          >
            <OsIcon os={h.os} size={grid ? 'md' : 'sm'} />
            <span className={cn('min-w-0 flex-1', !grid && 'flex items-center gap-4')}>
              <span className={cn('flex items-center gap-1.5', !grid && 'w-56 shrink-0')}>
                <span className="truncate text-[14px] font-extrabold">{h.label}</span>
                {h.favorite ? (
                  <Star
                    className="text-warning size-3 shrink-0 fill-current"
                    aria-label={t('hosts.favorite')}
                  />
                ) : null}
              </span>
              <span
                className={cn(
                  'text-muted block truncate font-mono text-[11.5px]',
                  grid ? 'mt-0.5' : 'min-w-0 flex-1',
                )}
              >
                {hostTarget(h)}
              </span>
              {grid ? (
                <span className="mt-2 flex min-w-0 items-center gap-1 overflow-hidden">
                  <ProtocolBadge protocol={protocol} />
                  {h.tags.slice(0, 3).map((tag) => (
                    <Badge key={tag}>{tag}</Badge>
                  ))}
                  {h.tags.length > 3 ? (
                    <span className="text-subtle text-[11px]">+{h.tags.length - 3}</span>
                  ) : null}
                </span>
              ) : (
                <span className="hidden min-w-0 items-center gap-1 overflow-hidden lg:flex">
                  {h.tags.slice(0, 3).map((tag) => (
                    <Badge key={tag}>{tag}</Badge>
                  ))}
                </span>
              )}
            </span>
            {!grid ? <ProtocolBadge protocol={protocol} /> : null}
          </button>
          {phone ? (
            <button
              type="button"
              aria-label={t('common.edit')}
              onClick={() => select({ kind: 'host', id: h.id })}
              className="text-subtle hover:text-text absolute top-1/2 right-1 flex size-11 -translate-y-1/2 items-center justify-center"
            >
              <Pencil className="size-4" />
            </button>
          ) : (
            <button
              type="button"
              tabIndex={-1}
              aria-label={`${t('hosts.connect')} ${h.label}`}
              onClick={() => sessions.connectHost(h)}
              className={cn(
                'bg-accent text-accent-fg hover:bg-accent-hover absolute right-3.5 flex size-8 items-center justify-center rounded-full opacity-0 shadow-[0_4px_14px_-4px_var(--accent)] transition-[opacity,transform] duration-150 group-focus-within/card:opacity-100 group-hover/card:opacity-100',
                grid ? 'top-3.5' : 'top-1/2 -translate-y-1/2',
              )}
            >
              <Play className="size-3.5 fill-current" />
            </button>
          )}
        </div>
      </ContextMenu>
    );
  };

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="px-4 pt-4 sm:px-8 sm:pt-7">
        {!phone ? (
          <PageHeader
            title={t('hosts.hosts')}
            description={t('hosts.summary', {
              hosts: t('hosts.hostCount', { count: hosts.length }),
              groups: t('hosts.groupCount', { count: groups.length }),
            })}
            actions={
              <>
                {platform.localTerminal ? (
                  <Button
                    variant="secondary"
                    onClick={() => sessions.openLocal()}
                    title={formatShortcut(platform, 'Mod+L')}
                  >
                    <SquareTerminal className="size-4" />
                    {t('hosts.terminal')}
                  </Button>
                ) : null}
                <Button variant="secondary" onClick={() => sessions.openSerialPicker()}>
                  <Usb className="size-4" />
                  {t('hosts.serial')}
                </Button>
                <CreateButton onHost={createHost} onGroup={createGroup} onImport={onImport} />
              </>
            }
          />
        ) : null}

        <div className={cn('flex items-center gap-2', !phone && 'mt-6')}>
          <form onSubmit={onSubmit} className="relative min-w-0 flex-1">
            <Search className="text-subtle pointer-events-none absolute top-1/2 left-4 size-4 -translate-y-1/2" />
            <input
              ref={inputRef}
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder={t('hosts.omnibox')}
              aria-label={t('hosts.omnibox')}
              className="bg-surface-3 border-border-input text-text placeholder:text-subtle hover:border-border-strong focus:border-accent focus:ring-accent-soft h-11 w-full rounded-full border pr-32 pl-11 text-[14px] font-semibold transition-[border-color,box-shadow] duration-150 focus:ring-3 focus:outline-none"
              autoCapitalize="off"
              autoCorrect="off"
              spellCheck={false}
            />
            <Button
              type="submit"
              variant={quick ? 'accent' : 'ghost'}
              size="sm"
              disabled={!quick && visibleHosts.length === 0}
              className="absolute top-1/2 right-1.5 h-9 -translate-y-1/2"
            >
              {t('hosts.connect')}
              <ArrowRight className="size-3.5" />
            </Button>
          </form>
          <div className="flex shrink-0 items-center gap-1.5">
            <DropdownMenu
              trigger={
                <Button
                  size={tagFilter.length ? 'md' : 'icon'}
                  variant={tagFilter.length ? 'accent' : 'secondary'}
                  aria-label={t('hosts.filterTags')}
                  className="h-11 min-w-11"
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
            <Segmented
              ariaLabel={t('hosts.layout')}
              value={hostView}
              onChange={setHostView}
              options={[
                {
                  value: 'grid',
                  label: <span className="sr-only">{t('hosts.gridView')}</span>,
                  icon: <LayoutGrid />,
                },
                {
                  value: 'list',
                  label: <span className="sr-only">{t('hosts.listView')}</span>,
                  icon: <List />,
                },
              ]}
              className="h-11 [&_button]:h-9 [&_button]:px-3"
            />
          </div>
        </div>
        {quick ? (
          <p className="text-muted mt-2 flex items-center gap-1.5 text-xs">
            {t('hosts.quickConnectHint', { target: quickTarget })} <Kbd>↵</Kbd>
          </p>
        ) : null}

        {phone || path.length > 0 || tagFilter.length > 0 ? (
          <div className="mt-4 flex flex-wrap items-center gap-2">
            {phone ? (
              <CreateButton onHost={createHost} onGroup={createGroup} onImport={onImport} />
            ) : null}
            {path.length > 0 ? (
              <nav
                aria-label={t('hosts.breadcrumb')}
                className="text-muted flex min-w-0 flex-wrap items-center gap-1 text-[13px]"
              >
                <button
                  type="button"
                  className="hover:text-text hover:bg-text/[0.07] rounded-full px-2 py-0.5 font-semibold transition-colors"
                  onClick={() => setCurrentGroup(null)}
                  onDragOver={(e) => e.preventDefault()}
                  onDrop={(e) => dropOnGroup(e, null)}
                >
                  {t('hosts.allHosts')}
                </button>
                {path.map((g) => (
                  <span key={g.id} className="flex items-center gap-1">
                    <ChevronRight className="text-subtle size-3.5" />
                    <button
                      type="button"
                      className={cn(
                        'hover:text-text hover:bg-surface-3 rounded-md px-1.5 py-0.5 transition-colors',
                        g.id === currentGroup && 'text-text font-semibold',
                      )}
                      onClick={() => setCurrentGroup(g.id)}
                    >
                      {g.label}
                    </button>
                  </span>
                ))}
              </nav>
            ) : null}
            {tagFilter.map((tag) => (
              <button
                key={tag}
                type="button"
                aria-label={`${t('hosts.clearTags')}: ${tag}`}
                onClick={() => setTagFilter(tagFilter.filter((x) => x !== tag))}
              >
                <Badge tone="accent">
                  <Tag className="size-3" />
                  {tag} ×
                </Badge>
              </button>
            ))}
          </div>
        ) : null}
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto px-4 pt-5 pb-8 sm:px-8">
        {subgroups.length > 0 && (
          <>
            <SectionHeader>{t('hosts.groups')}</SectionHeader>
            <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 xl:grid-cols-3 2xl:grid-cols-4">
              {subgroups.map((g) => (
                <ContextMenu key={g.id} items={groupMenu(g)}>
                  <button
                    type="button"
                    onClick={() => select({ kind: 'group', id: g.id })}
                    onDoubleClick={() => setCurrentGroup(g.id)}
                    onDragOver={(e) => e.preventDefault()}
                    onDrop={(e) => dropOnGroup(e, g.id)}
                    className={cn(
                      'bg-surface-2 border-border hover:border-border-strong hover:bg-text/[0.04] group flex h-16 items-center gap-3 rounded-[var(--radius-card)] border px-3.5 text-left transition-[background-color,border-color,box-shadow] duration-150',
                      selected?.id === g.id && 'border-accent! ring-accent-soft ring-3',
                    )}
                  >
                    <GroupIcon />
                    <span className="min-w-0 flex-1">
                      <span className="block truncate text-[14px] font-extrabold">{g.label}</span>
                      <span className="text-muted block text-xs">
                        {t('hosts.hostCount', { count: counts.get(g.id) ?? 0 })}
                      </span>
                    </span>
                    <ChevronRight
                      className="text-subtle size-4 transition-transform duration-150 group-hover:translate-x-0.5"
                      aria-label={phone ? t('hosts.openGroup') : undefined}
                      onClick={(e) => {
                        e.stopPropagation();
                        setCurrentGroup(g.id);
                      }}
                    />
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
                  className="text-accent-text ml-2 font-medium"
                  onClick={() => setMulti(new Set())}
                >
                  {t('common.clear')}
                </button>
              </span>
            ) : (
              <span className="text-subtle font-mono text-[11px]">{visibleHosts.length}</span>
            )
          }
        >
          {t('hosts.hosts')}
        </SectionHeader>
        {visibleHosts.length === 0 ? (
          <EmptyState
            icon={filtering ? <Search /> : <Server />}
            title={filtering ? t('hosts.noMatches') : t('hosts.emptyTitle')}
            description={filtering ? undefined : t('hosts.emptyDescription')}
            action={
              filtering ? undefined : (
                <div className="flex gap-2">
                  <Button variant="accent" onClick={createHost}>
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
                ? 'grid grid-cols-1 gap-3 sm:grid-cols-2 xl:grid-cols-3 2xl:grid-cols-4'
                : 'flex flex-col gap-1.5',
            )}
          >
            {visibleHosts.map(hostCard)}
          </div>
        )}
      </div>
    </div>
  );
}

function ProtocolBadge({ protocol }: { protocol: 'ssh' | 'mosh' | 'telnet' }) {
  return (
    <Badge tone={protocol === 'ssh' ? 'accent' : protocol === 'mosh' ? 'info' : 'warning'}>
      {protocol}
    </Badge>
  );
}

/** "New host" with a menu for the other things that can be created. */
function CreateButton({
  onHost,
  onGroup,
  onImport,
}: {
  onHost: () => void;
  onGroup: () => void;
  onImport: () => void;
}) {
  const { t } = useTranslation();
  return (
    <div className="flex">
      <Button variant="primary" onClick={onHost} className="rounded-r-none pr-3.5">
        <Plus className="size-4" />
        {t('hosts.newHost')}
      </Button>
      <DropdownMenu
        align="end"
        trigger={
          <Button
            variant="primary"
            className="border-primary-fg/15 rounded-l-none border-l pr-3 pl-2.5"
            aria-label={t('hosts.moreCreate')}
          >
            <ChevronDown className="size-4" />
          </Button>
        }
        items={[
          { label: t('hosts.newGroup'), icon: <FolderPlus />, onSelect: onGroup },
          { label: t('hosts.import'), icon: <Upload />, onSelect: onImport },
        ]}
      />
    </div>
  );
}
