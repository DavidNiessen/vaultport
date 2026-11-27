// SPDX-License-Identifier: GPL-3.0-or-later
/** Dual-pane SFTP file manager (local ↔ remote and remote ↔ remote). */
import { useQuery, useQueryClient } from '@tanstack/react-query';
import {
  ArrowLeft,
  ArrowRight,
  ChevronRight,
  Copy,
  Download,
  Eye,
  EyeOff,
  File,
  FileSymlink,
  Folder,
  FolderPlus,
  HardDrive,
  Pencil,
  RefreshCw,
  Search,
  Trash2,
  X,
} from 'lucide-react';
import { useMemo, useState, type DragEvent } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';

import { OsIcon } from '@/components/OsIcon';
import { Button } from '@/components/ui/button';
import { Select } from '@/components/ui/controls';
import { Input } from '@/components/ui/input';
import { EmptyState } from '@/components/ui/misc';
import { ContextMenu, DropdownMenu } from '@/components/ui/overlay';
import { cn } from '@/lib/cn';
import { EMPTY } from '@/lib/empty';
import { commands, errorMessage, type FileEntry, type Transfer } from '@/lib/ipc';
import { useBreakpoint } from '@/lib/platform';
import { useHosts } from '@/lib/queries';

import { useSftpStore, type PaneSource } from './store';

const DRAG = 'application/x-vaultport-files';

export function formatSize(n: number | null): string {
  if (n === null) return '—';
  if (n < 1024) return `${String(n)} B`;
  const u = ['KB', 'MB', 'GB', 'TB'];
  let v = n / 1024;
  let i = 0;
  while (v >= 1024 && i < u.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v.toFixed(v < 10 ? 1 : 0)} ${u[i] ?? 'TB'}`;
}

function sourceKey(s: PaneSource): string {
  return s.kind === 'local' ? 'local' : s.hostId;
}

/** Splits a POSIX or Windows path into breadcrumb segments with their full paths. */
export function breadcrumbs(path: string): { name: string; path: string }[] {
  const win = /^[A-Za-z]:[\\/]/.test(path);
  const sep = win ? '\\' : '/';
  const parts = path.split(/[\\/]/).filter(Boolean);
  const out: { name: string; path: string }[] = [];
  let acc = win ? '' : '/';
  if (!win) out.push({ name: '/', path: '/' });
  for (const p of parts) {
    acc = win && acc === '' ? `${p}${sep}` : `${acc}${acc.endsWith(sep) ? '' : sep}${p}`;
    out.push({ name: p, path: acc });
  }
  return out;
}

function Pane({ side, other }: { side: 'left' | 'right'; other: PaneSource | null }) {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const hosts = useHosts().data ?? EMPTY;
  const source = useSftpStore((s) => (side === 'left' ? s.left : s.right));
  const setPane = useSftpStore((s) => s.setPane);
  const [history, setHistory] = useState<string[]>([]);
  const [forward, setForward] = useState<string[]>([]);
  const [filter, setFilter] = useState('');
  const [showHidden, setShowHidden] = useState(false);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [editingPath, setEditingPath] = useState<string | null>(null);
  const [sort, setSort] = useState<{ key: 'name' | 'modified' | 'size'; asc: boolean }>({
    key: 'name',
    asc: true,
  });

  const listing = useQuery({
    queryKey: ['sftp', side, source ? sourceKey(source) : 'none', source?.path ?? null],
    queryFn: () =>
      source ? commands.fsList(toSource(source), source.path) : Promise.resolve(null),
    enabled: Boolean(source),
    retry: false,
  });
  const path = listing.data?.path ?? source?.path ?? '';
  const entries = useMemo(() => {
    const list = (listing.data?.entries ?? [])
      .filter((e) => showHidden || !e.hidden)
      .filter((e) => !filter || e.name.toLowerCase().includes(filter.toLowerCase()));
    const dir = (e: FileEntry) => (e.kind === 'dir' ? 0 : 1);
    return [...list].sort((a, b) => {
      const d = dir(a) - dir(b);
      if (d !== 0) return d;
      const v =
        sort.key === 'name'
          ? a.name.localeCompare(b.name, undefined, { numeric: true, sensitivity: 'base' })
          : sort.key === 'size'
            ? (a.size ?? 0) - (b.size ?? 0)
            : (a.modified ?? 0) - (b.modified ?? 0);
      return sort.asc ? v : -v;
    });
  }, [listing.data, showHidden, filter, sort]);

  if (!source) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-3 p-6">
        <EmptyState
          icon={<HardDrive />}
          title={t('sftp.chooseHost')}
          description={t('sftp.chooseHostHint')}
        />
        <Select
          aria-label={t('sftp.chooseHost')}
          className="w-64"
          value=""
          onChange={(e) =>
            setPane(
              side,
              e.target.value === 'local'
                ? { kind: 'local', path: null }
                : { kind: 'remote', hostId: e.target.value, path: null },
            )
          }
        >
          <option value="">{t('sftp.select')}</option>
          <option value="local">{t('sftp.local')}</option>
          {hosts.map((h) => (
            <option key={h.id} value={h.id}>
              {h.label}
            </option>
          ))}
        </Select>
      </div>
    );
  }

  const navigate = (p: string) => {
    setHistory((h) => [...h, path]);
    setForward([]);
    setSelected(new Set());
    setPane(side, { ...source, path: p });
  };
  const refresh = () => qc.invalidateQueries({ queryKey: ['sftp', side] });
  const join = (name: string) =>
    path.endsWith('/') || path.endsWith('\\')
      ? `${path}${name}`
      : `${path}${path.includes('\\') && !path.includes('/') ? '\\' : '/'}${name}`;
  const selectedEntries = entries.filter((e) => selected.has(e.path));

  const transfer = (items: FileEntry[]) => {
    if (!other?.path && other?.kind !== 'local') {
      toast.error(t('sftp.noTarget'));
      return;
    }
    void commands
      .transferStart(
        toSource(source),
        items.map((e) => e.path),
        toSource(other),
        other.path ?? '',
        'ask',
      )
      .then(() => toast.success(t('sftp.transferQueued', { count: items.length })))
      .catch((e: unknown) => toast.error(errorMessage(e)));
  };

  const act = async (fn: () => Promise<unknown>) => {
    try {
      await fn();
      await refresh();
    } catch (e) {
      toast.error(errorMessage(e));
    }
  };

  const onDrop = (e: DragEvent) => {
    e.preventDefault();
    const raw = e.dataTransfer.getData(DRAG);
    if (!raw) return;
    const payload = JSON.parse(raw) as { side: string; source: PaneSource; paths: string[] };
    if (payload.side === side) return;
    void commands
      .transferStart(toSource(payload.source), payload.paths, toSource(source), path, 'ask')
      .then(() => toast.success(t('sftp.transferQueued', { count: payload.paths.length })))
      .catch((err: unknown) => toast.error(errorMessage(err)));
  };

  const host = source.kind === 'remote' ? hosts.find((h) => h.id === source.hostId) : null;
  const sortBtn = (key: typeof sort.key, label: string, cls: string) => (
    <button
      type="button"
      className={cn('hover:text-text text-left', cls)}
      onClick={() => setSort((s) => ({ key, asc: s.key === key ? !s.asc : true }))}
    >
      {label}
      {sort.key === key ? (sort.asc ? ' ↑' : ' ↓') : ''}
    </button>
  );

  return (
    <section
      aria-label={host?.label ?? t('sftp.local')}
      className="flex h-full min-h-0 flex-col"
      onDragOver={(e) => e.preventDefault()}
      onDrop={onDrop}
    >
      <header className="flex items-center gap-2 px-3 pt-3">
        {host ? (
          <OsIcon os={host.os} size="sm" />
        ) : (
          <div className="bg-accent-soft text-accent-text flex size-7 items-center justify-center rounded-lg">
            <HardDrive className="size-3.5" />
          </div>
        )}
        <span className="min-w-0 flex-1 truncate text-[13px] font-semibold">
          {host?.label ?? t('sftp.local')}
        </span>
        <div className="relative">
          <Search className="text-subtle absolute top-1/2 left-2 size-3.5 -translate-y-1/2" />
          <Input
            aria-label={t('sftp.filter')}
            placeholder={t('sftp.filter')}
            className="h-8 w-32 pl-7 text-[13px]"
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
          />
        </div>
        <DropdownMenu
          trigger={
            <Button size="sm" variant="ghost">
              {t('sftp.actions')}
            </Button>
          }
          items={[
            {
              label: showHidden ? t('sftp.hideHidden') : t('sftp.showHidden'),
              icon: showHidden ? <EyeOff /> : <Eye />,
              onSelect: () => setShowHidden(!showHidden),
            },
            {
              label: t('sftp.newFolder'),
              icon: <FolderPlus />,
              onSelect: () => {
                const n = window.prompt(t('sftp.folderName'));
                if (n) void act(() => commands.fsMkdir(toSource(source), join(n)));
              },
            },
            { label: t('sftp.refresh'), icon: <RefreshCw />, onSelect: () => void refresh() },
            { separator: true },
            {
              label: t('sftp.changeSource'),
              icon: <X />,
              onSelect: () => setPane(side, side === 'left' ? { kind: 'local', path: null } : null),
            },
          ]}
        />
      </header>
      <div className="flex items-center gap-1 px-3 py-2">
        <Button
          size="icon-sm"
          variant="ghost"
          aria-label={t('sftp.back')}
          disabled={history.length === 0}
          onClick={() => {
            const prev = history[history.length - 1];
            if (prev === undefined) return;
            setForward((f) => [path, ...f]);
            setHistory((h) => h.slice(0, -1));
            setPane(side, { ...source, path: prev });
          }}
        >
          <ArrowLeft className="size-4" />
        </Button>
        <Button
          size="icon-sm"
          variant="ghost"
          aria-label={t('sftp.forward')}
          disabled={forward.length === 0}
          onClick={() => {
            const next = forward[0];
            if (next === undefined) return;
            setForward((f) => f.slice(1));
            setHistory((h) => [...h, path]);
            setPane(side, { ...source, path: next });
          }}
        >
          <ArrowRight className="size-4" />
        </Button>
        {editingPath !== null ? (
          <Input
            autoFocus
            aria-label={t('sftp.path')}
            className="h-8 flex-1 font-mono text-[12px]"
            value={editingPath}
            onChange={(e) => setEditingPath(e.target.value)}
            onBlur={() => setEditingPath(null)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') {
                navigate(editingPath);
                setEditingPath(null);
              }
              if (e.key === 'Escape') setEditingPath(null);
            }}
          />
        ) : (
          <nav
            aria-label={t('sftp.path')}
            className="flex min-w-0 flex-1 cursor-text [scrollbar-width:none] items-center overflow-x-auto text-[13px]"
            onDoubleClick={() => setEditingPath(path)}
          >
            {breadcrumbs(path).map((c, i, arr) => (
              <span key={c.path} className="flex shrink-0 items-center">
                {i > 0 && c.name !== '/' ? <ChevronRight className="text-subtle size-3.5" /> : null}
                <button
                  type="button"
                  className={cn(
                    'hover:bg-surface-3 rounded px-1.5 py-0.5',
                    i === arr.length - 1 ? 'text-text font-medium' : 'text-muted',
                  )}
                  onClick={() => (i === arr.length - 1 ? void refresh() : navigate(c.path))}
                >
                  {c.name}
                </button>
              </span>
            ))}
          </nav>
        )}
      </div>
      <div className="text-subtle grid grid-cols-[1fr_110px_70px] gap-2 border-y border-[var(--border)] px-4 py-1.5 text-[11px] font-medium tracking-wide uppercase sm:grid-cols-[1fr_150px_80px]">
        {sortBtn('name', t('sftp.name'), '')}
        {sortBtn('modified', t('sftp.modified'), '')}
        {sortBtn('size', t('sftp.size'), 'text-right')}
      </div>
      <div
        className="min-h-0 flex-1 overflow-y-auto px-1.5 py-1"
        role="listbox"
        aria-multiselectable
        aria-label={t('sftp.files')}
      >
        {listing.isError ? (
          <p className="text-danger p-4 text-[13px]">{errorMessage(listing.error)}</p>
        ) : null}
        {listing.isLoading ? (
          <p className="text-subtle p-4 text-[13px]">{t('common.loading')}</p>
        ) : null}
        {entries.map((e) => (
          <ContextMenu
            key={e.path}
            items={[
              ...(e.kind === 'dir'
                ? [{ label: t('sftp.open'), onSelect: () => navigate(e.path) }]
                : [
                    {
                      label: t('sftp.edit'),
                      icon: <Pencil />,
                      onSelect: () =>
                        void commands
                          .fsEdit(toSource(source), e.path)
                          .catch((err: unknown) => toast.error(errorMessage(err))),
                    },
                  ]),
              {
                label: t('sftp.copyToTarget'),
                icon: <Copy />,
                onSelect: () => transfer(selected.has(e.path) ? selectedEntries : [e]),
              },
              ...(source.kind === 'remote' && e.kind !== 'dir'
                ? [
                    {
                      label: t('sftp.download'),
                      icon: <Download />,
                      onSelect: () =>
                        void commands
                          .fsDownload(toSource(source), e.path)
                          .catch((err: unknown) => toast.error(errorMessage(err))),
                    },
                  ]
                : []),
              {
                label: t('sftp.rename'),
                icon: <Pencil />,
                onSelect: () => {
                  const n = window.prompt(t('sftp.newName'), e.name);
                  if (n && n !== e.name)
                    void act(() => commands.fsRename(toSource(source), e.path, join(n)));
                },
              },
              {
                label: t('sftp.permissions'),
                onSelect: () => {
                  const m = window.prompt(
                    t('sftp.permissionsPrompt'),
                    (e.mode & 0o7777).toString(8),
                  );
                  if (m && /^[0-7]{3,4}$/.test(m))
                    void act(() => commands.fsChmod(toSource(source), e.path, parseInt(m, 8)));
                },
              },
              { separator: true },
              {
                label: t('common.delete'),
                icon: <Trash2 />,
                danger: true,
                onSelect: () => {
                  if (
                    window.confirm(
                      t('sftp.confirmDelete', { count: selected.has(e.path) ? selected.size : 1 }),
                    )
                  )
                    void act(async () => {
                      for (const x of selected.has(e.path) ? selectedEntries : [e])
                        await commands.fsRemove(toSource(source), x.path, x.kind === 'dir');
                    });
                },
              },
            ]}
          >
            <div
              role="option"
              aria-selected={selected.has(e.path)}
              tabIndex={0}
              draggable
              onDragStart={(ev) => {
                const paths = selected.has(e.path) ? [...selected] : [e.path];
                ev.dataTransfer.setData(DRAG, JSON.stringify({ side, source, paths }));
              }}
              onClick={(ev) => {
                setSelected((s) => {
                  const n = new Set(ev.metaKey || ev.ctrlKey ? s : []);
                  if (n.has(e.path)) n.delete(e.path);
                  else n.add(e.path);
                  return n;
                });
              }}
              onDoubleClick={() =>
                e.kind === 'dir'
                  ? navigate(e.path)
                  : void commands
                      .fsEdit(toSource(source), e.path)
                      .catch((err: unknown) => toast.error(errorMessage(err)))
              }
              onKeyDown={(ev) => {
                if (ev.key === 'Enter' && e.kind === 'dir') navigate(e.path);
              }}
              className={cn(
                'grid grid-cols-[1fr_110px_70px] items-center gap-2 rounded-lg px-2.5 py-1.5 sm:grid-cols-[1fr_150px_80px]',
                selected.has(e.path) ? 'bg-accent-soft' : 'hover:bg-surface-2',
                e.hidden && 'opacity-60',
              )}
            >
              <div className="flex min-w-0 items-center gap-2.5">
                {e.kind === 'dir' ? (
                  <Folder className="text-info size-4 shrink-0 fill-current/20" />
                ) : e.kind === 'symlink' ? (
                  <FileSymlink className="text-muted size-4 shrink-0" />
                ) : (
                  <File className="text-muted size-4 shrink-0" />
                )}
                <div className="min-w-0">
                  <div className="truncate text-[13px]">{e.name}</div>
                  <div className="text-subtle font-mono text-[10.5px]">{e.permissions}</div>
                </div>
              </div>
              <div className="text-subtle truncate text-xs">
                {e.modified
                  ? new Date(e.modified).toLocaleString(undefined, {
                      dateStyle: 'short',
                      timeStyle: 'short',
                    })
                  : '—'}
              </div>
              <div className="text-subtle text-right text-xs">
                {e.kind === 'dir' ? '—' : formatSize(e.size)}
              </div>
            </div>
          </ContextMenu>
        ))}
      </div>
    </section>
  );
}

function toSource(s: PaneSource) {
  return s.kind === 'local'
    ? ({ type: 'local' } as const)
    : ({ type: 'remote', hostId: s.hostId } as const);
}

function Transfers() {
  const { t } = useTranslation();
  const transfers =
    useQuery({
      queryKey: ['transfers'],
      queryFn: () => commands.transferList(),
      refetchInterval: 700,
    }).data ?? EMPTY;
  const active = transfers.filter((x: Transfer) => x.state !== 'done');
  if (transfers.length === 0) return null;
  return (
    <div className="border-border bg-surface-1 max-h-40 overflow-y-auto border-t px-4 py-2">
      <div className="mb-1 flex items-center justify-between">
        <span className="text-muted text-[12px] font-medium">
          {t('sftp.transfers', { count: active.length })}
        </span>
        <Button size="sm" variant="ghost" onClick={() => void commands.transferClearFinished()}>
          {t('sftp.clearFinished')}
        </Button>
      </div>
      {transfers.map((x: Transfer) => (
        <div key={x.id} className="flex items-center gap-3 py-1 text-xs">
          <span className="min-w-0 flex-1 truncate">{x.name}</span>
          <span className="text-subtle w-24 text-right">
            {x.state === 'running' ? `${formatSize(x.rate)}/s` : t(`sftp.state.${x.state}`)}
          </span>
          <div className="bg-surface-4 h-1.5 w-32 overflow-hidden rounded-full">
            <div
              className={cn('h-full', x.state === 'failed' ? 'bg-danger' : 'bg-accent')}
              style={{
                width: `${String(x.total ? Math.round(((x.done ?? 0) / x.total) * 100) : 0)}%`,
              }}
            />
          </div>
          {x.state === 'running' || x.state === 'queued' ? (
            <button
              type="button"
              aria-label={t('common.cancel')}
              className="text-subtle hover:text-danger"
              onClick={() => void commands.transferCancel(x.id)}
            >
              <X className="size-3.5" />
            </button>
          ) : null}
        </div>
      ))}
    </div>
  );
}

export function SftpView() {
  const bp = useBreakpoint();
  const left = useSftpStore((s) => s.left);
  const right = useSftpStore((s) => s.right);
  const [mobilePane, setMobilePane] = useState<'left' | 'right'>('right');
  const { t } = useTranslation();
  if (bp === 'phone') {
    return (
      <div className="flex h-full min-h-0 flex-col">
        <div className="flex gap-1 px-3 pt-3">
          {(['left', 'right'] as const).map((s) => (
            <Button
              key={s}
              size="sm"
              variant={mobilePane === s ? 'primary' : 'ghost'}
              onClick={() => setMobilePane(s)}
            >
              {s === 'left' ? t('sftp.local') : t('sftp.remote')}
            </Button>
          ))}
        </div>
        <div className="min-h-0 flex-1">
          <Pane side={mobilePane} other={mobilePane === 'left' ? right : left} />
        </div>
        <Transfers />
      </div>
    );
  }
  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="divide-border grid min-h-0 flex-1 grid-cols-2 divide-x">
        <Pane side="left" other={right} />
        <Pane side="right" other={left} />
      </div>
      <Transfers />
    </div>
  );
}
