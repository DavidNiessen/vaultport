// SPDX-License-Identifier: GPL-3.0-or-later
/**
 * The custom title bar: OS-appropriate window controls, pinned Vault and SFTP tabs, session tabs
 * (drag to reorder, middle-click to close), new-tab and command-palette buttons. The whole bar
 * is a window drag region; double-clicking it toggles maximize (handled by Tauri).
 */
import {
  Copy,
  FolderOpen,
  History,
  LayoutGrid,
  PanelRight,
  Pencil,
  Plus,
  Search,
  SquareTerminal,
  Vault,
  X,
} from 'lucide-react';
import { useState, type DragEvent, type MouseEvent } from 'react';
import { useTranslation } from 'react-i18next';

import { Kbd, Tooltip } from '@/components/ui/controls';
import { ContextMenu, type MenuItemDef } from '@/components/ui/overlay';
import { cn } from '@/lib/cn';
import { formatShortcut, usePlatform } from '@/lib/platform';
import { SFTP_TAB, VAULT_TAB, useApp, type Tab } from '@/stores/app';

import { primaryBinding, useKeymap, type ShortcutId } from '../features/palette/shortcuts';
import { duplicate } from '../features/terminal/tabActions';

import { LinuxControls, WindowsControls, useWindowControls } from './WindowControls';

function statusColor(tab: Tab): string | null {
  if (tab.kind !== 'terminal') return null;
  switch (tab.status) {
    case 'connecting':
      return 'bg-warning animate-pulse';
    case 'connected':
      return tab.unread ? 'bg-info' : 'bg-success';
    case 'error':
      return 'bg-danger';
    case 'disconnected':
      return 'bg-subtle';
  }
}

function TabIcon({ tab }: { tab: Tab }) {
  const cls = 'size-3.5 shrink-0';
  switch (tab.kind) {
    case 'terminal':
      return <SquareTerminal className={cls} />;
    case 'workspace':
      return <LayoutGrid className={cls} />;
    case 'log':
      return <History className={cls} />;
    case 'sftp':
      return <FolderOpen className={cls} />;
  }
}

function PinnedTab({
  id,
  label,
  icon,
  active,
}: {
  id: string;
  label: string;
  icon: React.ReactNode;
  active: boolean;
}) {
  const activate = useApp((s) => s.activate);
  return (
    <button
      type="button"
      role="tab"
      aria-selected={active}
      onClick={() => activate(id)}
      className={cn(
        'no-drag flex h-8 shrink-0 items-center gap-2 rounded-full px-3 text-[13px] font-bold transition-colors duration-150',
        active ? 'bg-text/[0.1] text-text' : 'text-muted hover:bg-text/[0.06] hover:text-text',
      )}
    >
      {icon}
      <span className="hidden sm:inline">{label}</span>
    </button>
  );
}

function SessionTab({ tab, index, active }: { tab: Tab; index: number; active: boolean }) {
  const { t } = useTranslation();
  const platform = usePlatform();
  const keymap = useKeymap((s) => s.keymap);
  const { activate, closeTab, moveTab, renaming, setRenaming, renameTab } = useApp();
  const [dragOver, setDragOver] = useState(false);
  const dot = statusColor(tab);
  const editing = renaming === tab.id;
  const hint = (id: ShortcutId) => {
    const k = primaryBinding(keymap, id);
    return k ? formatShortcut(platform, k) : undefined;
  };

  const onAuxClick = (e: MouseEvent) => {
    if (e.button === 1) {
      e.preventDefault();
      closeTab(tab.id);
    }
  };
  const onDragStart = (e: DragEvent) => {
    e.dataTransfer.setData('application/x-vaultport-tab', String(index));
    e.dataTransfer.effectAllowed = 'move';
  };
  const onDrop = (e: DragEvent) => {
    const from = Number(e.dataTransfer.getData('application/x-vaultport-tab'));
    setDragOver(false);
    if (!Number.isNaN(from) && from !== index) moveTab(from, index);
  };

  const s = useApp.getState;
  const items: MenuItemDef[] = [
    {
      label: t('tabs.rename'),
      icon: <Pencil className="size-4" />,
      shortcut: hint('renameTab'),
      onSelect: () => setRenaming(tab.id),
    },
    ...(tab.kind === 'terminal'
      ? [
          {
            label: t('tabs.duplicate'),
            icon: <Copy className="size-4" />,
            shortcut: hint('duplicateTab'),
            onSelect: () => duplicate(tab.id),
          },
        ]
      : []),
    { separator: true },
    {
      label: t('tabs.closeTab'),
      icon: <X className="size-4" />,
      shortcut: hint('closeTab'),
      onSelect: () => closeTab(tab.id),
    },
    { label: t('tabs.closeOthers'), onSelect: () => s().closeOthers(tab.id) },
    { label: t('tabs.closeRight'), onSelect: () => s().closeToRight(tab.id) },
    {
      label: t('tabs.reopen'),
      shortcut: hint('reopenTab'),
      disabled: s().closedTabs.length === 0,
      onSelect: () => s().reopenClosed(),
    },
  ];

  return (
    <ContextMenu items={items}>
      <div
        role="tab"
        aria-selected={active}
        tabIndex={0}
        draggable={!editing}
        onDragStart={onDragStart}
        onDragOver={(e) => {
          e.preventDefault();
          setDragOver(true);
        }}
        onDragLeave={() => setDragOver(false)}
        onDrop={onDrop}
        onClick={() => activate(tab.id)}
        onDoubleClick={() => setRenaming(tab.id)}
        onKeyDown={(e) => {
          if (editing) return;
          if (e.key === 'Enter' || e.key === ' ') activate(tab.id);
          if (e.key === 'F2') setRenaming(tab.id);
        }}
        onAuxClick={onAuxClick}
        title={tab.title}
        className={cn(
          'no-drag group relative flex h-8 max-w-56 min-w-28 shrink items-center gap-2 rounded-full pr-1.5 pl-3 text-[13px] font-bold transition-colors duration-150',
          active ? 'bg-text/[0.1] text-text' : 'text-muted hover:bg-text/[0.06] hover:text-text',
          dragOver && 'ring-accent ring-2',
        )}
      >
        <TabIcon tab={tab} />
        {editing ? (
          <input
            autoFocus
            aria-label={t('tabs.rename')}
            defaultValue={tab.title}
            onFocus={(e) => e.currentTarget.select()}
            onClick={(e) => e.stopPropagation()}
            onKeyDown={(e) => {
              e.stopPropagation();
              if (e.key === 'Enter') renameTab(tab.id, e.currentTarget.value);
              if (e.key === 'Escape') setRenaming(null);
            }}
            onBlur={(e) => renameTab(tab.id, e.currentTarget.value)}
            className="bg-surface-3 text-text focus:border-accent min-w-0 flex-1 rounded border border-transparent px-1 text-[13px] font-semibold outline-none"
          />
        ) : (
          <span className="min-w-0 flex-1 truncate">{tab.title}</span>
        )}
        {dot ? <span className={cn('size-1.5 shrink-0 rounded-full', dot)} aria-hidden /> : null}
        <button
          type="button"
          aria-label={t('tabs.close', { title: tab.title })}
          onClick={(e) => {
            e.stopPropagation();
            closeTab(tab.id);
          }}
          className={cn(
            'text-subtle hover:bg-text/[0.12] hover:text-text flex size-5 shrink-0 items-center justify-center rounded-full transition-opacity focus:opacity-100',
            active ? 'opacity-100' : 'opacity-0 group-hover:opacity-100',
          )}
        >
          <X className="size-3.5" />
        </button>
      </div>
    </ContextMenu>
  );
}

export function TitleBar() {
  const { t } = useTranslation();
  const platform = usePlatform();
  const controls = useWindowControls(platform);
  const { tabs, activeTab, setNewTabOpen, setPaletteOpen, sidePanelOpen, setSidePanelOpen } =
    useApp();
  const mac = platform.kind === 'macos';
  const keymap = useKeymap((s) => s.keymap);
  const fmt = (id: ShortcutId) => {
    const k = primaryBinding(keymap, id);
    return k ? formatShortcut(platform, k) : null;
  };
  const newTabHint = [t('tabs.new'), fmt('connect') ? `(${fmt('connect') ?? ''})` : '']
    .filter(Boolean)
    .join(' ');
  const activeKind = tabs.find((x) => x.id === activeTab)?.kind;

  return (
    <header
      data-tauri-drag-region
      className={cn(
        'drag-region border-border flex h-12 shrink-0 items-center gap-1 border-b bg-[var(--titlebar-bg)] select-none',
        mac ? 'pl-[84px]' : controls.start.length ? 'pl-0' : 'pl-2',
      )}
    >
      {controls.kind === 'linux' && controls.start.length > 0 ? (
        <LinuxControls actions={controls.start} />
      ) : null}

      <div data-tauri-drag-region className="flex min-w-0 flex-1 items-center gap-1">
        <div
          role="tablist"
          aria-label={t('tabs.label')}
          data-tauri-drag-region
          className="flex min-w-0 items-center gap-1"
        >
          <PinnedTab
            id={VAULT_TAB}
            label={t('tabs.vault')}
            icon={<Vault className="size-4" />}
            active={activeTab === VAULT_TAB}
          />
          <PinnedTab
            id={SFTP_TAB}
            label={t('tabs.sftp')}
            icon={<FolderOpen className="size-4" />}
            active={activeTab === SFTP_TAB}
          />
          {tabs.length > 0 && (
            <div className="bg-border-strong mx-1.5 h-4 w-px shrink-0" aria-hidden />
          )}
          <div
            data-tauri-drag-region
            className="flex min-w-0 [scrollbar-width:none] items-center gap-1 overflow-x-auto"
          >
            {tabs.map((tab, i) =>
              tab.kind === 'sftp' || (tab.kind === 'terminal' && tab.workspace) ? null : (
                <SessionTab key={tab.id} tab={tab} index={i} active={activeTab === tab.id} />
              ),
            )}
          </div>
        </div>
        <Tooltip content={newTabHint}>
          <button
            type="button"
            aria-label={t('tabs.new')}
            onClick={() => setNewTabOpen(true)}
            className="no-drag text-muted hover:bg-text/[0.08] hover:text-text flex size-8 shrink-0 items-center justify-center rounded-full transition-colors"
          >
            <Plus className="size-4" />
          </button>
        </Tooltip>
        <div data-tauri-drag-region className="h-full min-w-6 flex-1" />
      </div>

      <div className="no-drag flex items-center gap-1 pr-1.5">
        <button
          type="button"
          aria-label={t('palette.open')}
          onClick={() => setPaletteOpen(true)}
          className="text-muted hover:text-text bg-text/[0.07] hover:bg-text/[0.1] flex h-8 items-center gap-2 rounded-full pr-1.5 pl-3 text-[13px] font-semibold whitespace-nowrap transition-colors lg:w-72"
        >
          <Search className="size-4 shrink-0" />
          <span className="hidden flex-1 text-left lg:inline">{t('palette.short')}</span>
          {fmt('palette') ? <Kbd>{fmt('palette')}</Kbd> : null}
        </button>
        {activeKind === 'terminal' || activeKind === 'workspace' ? (
          <Tooltip
            content={[
              t('terminal.sidePanel'),
              fmt('sidePanel') ? `(${fmt('sidePanel') ?? ''})` : '',
            ]
              .filter(Boolean)
              .join(' ')}
          >
            <button
              type="button"
              aria-label={t('terminal.sidePanel')}
              aria-pressed={sidePanelOpen}
              onClick={() => setSidePanelOpen(!sidePanelOpen)}
              className={cn(
                'hover:bg-text/[0.08] hover:text-text flex size-8 items-center justify-center rounded-full transition-colors',
                sidePanelOpen ? 'text-accent-text bg-accent-soft' : 'text-muted',
              )}
            >
              <PanelRight className="size-4" />
            </button>
          </Tooltip>
        ) : null}
      </div>

      {controls.kind === 'windows' ? <WindowsControls /> : null}
      {controls.kind === 'linux' ? <LinuxControls actions={controls.end} /> : null}
    </header>
  );
}
