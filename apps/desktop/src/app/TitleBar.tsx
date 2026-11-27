// SPDX-License-Identifier: GPL-3.0-or-later
/**
 * The custom title bar: OS-appropriate window controls, pinned Vault and SFTP tabs, session tabs
 * (drag to reorder, middle-click to close), new-tab and command-palette buttons. The whole bar
 * is a window drag region; double-clicking it toggles maximize (handled by Tauri).
 */
import {
  FolderOpen,
  History,
  LayoutGrid,
  PanelRight,
  Plus,
  Search,
  SquareTerminal,
  Vault,
  X,
} from 'lucide-react';
import { useState, type DragEvent, type MouseEvent } from 'react';
import { useTranslation } from 'react-i18next';

import { Kbd, Tooltip } from '@/components/ui/controls';
import { cn } from '@/lib/cn';
import { formatShortcut, usePlatform } from '@/lib/platform';
import { SFTP_TAB, VAULT_TAB, useApp, type Tab } from '@/stores/app';

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
  const { activate, closeTab, moveTab } = useApp();
  const [dragOver, setDragOver] = useState(false);
  const dot = statusColor(tab);

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

  return (
    <div
      role="tab"
      aria-selected={active}
      tabIndex={0}
      draggable
      onDragStart={onDragStart}
      onDragOver={(e) => {
        e.preventDefault();
        setDragOver(true);
      }}
      onDragLeave={() => setDragOver(false)}
      onDrop={onDrop}
      onClick={() => activate(tab.id)}
      onKeyDown={(e) => {
        if (e.key === 'Enter' || e.key === ' ') activate(tab.id);
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
      <span className="min-w-0 flex-1 truncate">{tab.title}</span>
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
  );
}

export function TitleBar() {
  const { t } = useTranslation();
  const platform = usePlatform();
  const controls = useWindowControls(platform);
  const { tabs, activeTab, setNewTabOpen, setPaletteOpen, sidePanelOpen, setSidePanelOpen } =
    useApp();
  const mac = platform.kind === 'macos';
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
            {tabs
              .filter((x) => x.kind !== 'sftp')
              .map((tab, i) => (
                <SessionTab key={tab.id} tab={tab} index={i} active={activeTab === tab.id} />
              ))}
          </div>
        </div>
        <Tooltip content={`${t('tabs.new')} (${formatShortcut(platform, 'Mod+T')})`}>
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
          <Kbd>{formatShortcut(platform, 'Mod+K')}</Kbd>
        </button>
        {activeKind === 'terminal' || activeKind === 'workspace' ? (
          <Tooltip content={`${t('terminal.sidePanel')} (${formatShortcut(platform, 'Mod+.')})`}>
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
