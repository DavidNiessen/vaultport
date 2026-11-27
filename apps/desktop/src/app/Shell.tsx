// SPDX-License-Identifier: GPL-3.0-or-later
/**
 * The main window layout. Desktop and tablet use the custom title bar with tabs; phones use a
 * bottom tab bar (Vault · Connections · Settings) and a full-screen terminal.
 */
import { ArrowLeft, LayoutGrid, Plus, Server, Settings, SquareTerminal, X } from 'lucide-react';
import { useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { OsIcon } from '@/components/OsIcon';
import { Button } from '@/components/ui/button';
import { EmptyState } from '@/components/ui/misc';
import { Sheet } from '@/components/ui/overlay';
import { cn } from '@/lib/cn';
import { EMPTY } from '@/lib/empty';
import type { VaultStatus } from '@/lib/ipc';
import { isApple, useBreakpoint, usePlatform } from '@/lib/platform';
import { useGroups, useHosts } from '@/lib/queries';
import { SFTP_TAB, VAULT_TAB, useApp, type Tab } from '@/stores/app';

import { ForwardingView } from '../features/forwarding/ForwardingView';
import { GroupInspector } from '../features/hosts/GroupInspector';
import { HostInspector } from '../features/hosts/HostInspector';
import { HostsView } from '../features/hosts/HostsView';
import { ImportDialog } from '../features/import/ImportDialog';
import { KeychainView } from '../features/keychain/KeychainView';
import { KnownHostsView } from '../features/known-hosts/KnownHostsView';
import { LogViewer } from '../features/logs/LogViewer';
import { LogsView } from '../features/logs/LogsView';
import { CommandPalette } from '../features/palette/CommandPalette';
import { NewTabDialog } from '../features/palette/NewTabDialog';
import { SHORTCUTS, matches } from '../features/palette/shortcuts';
import { SettingsView } from '../features/settings/SettingsView';
import { SftpView } from '../features/sftp/SftpView';
import { SnippetsView } from '../features/snippets/SnippetsView';
import { TerminalSidePanel } from '../features/terminal/SidePanel';
import { TerminalView } from '../features/terminal/TerminalView';
import { useSessions } from '../features/terminal/sessions';
import { WorkspaceView } from '../features/workspaces/WorkspaceView';
import { TitleBar } from './TitleBar';
import { SECTIONS, VaultNav } from './VaultNav';

interface ShellProps {
  status: VaultStatus;
  onStatus: (s: VaultStatus) => void;
  onLock: () => void;
}

function useShortcuts(onLock: () => void) {
  const platform = usePlatform();
  const app = useApp();
  const sessions = useSessions();
  useEffect(() => {
    const apple = isApple(platform);
    const byId = Object.fromEntries(SHORTCUTS.map((s) => [s.id, s.keys]));
    const visible = () => [
      VAULT_TAB,
      SFTP_TAB,
      ...useApp
        .getState()
        .tabs.filter((x) => !(x.kind === 'terminal' && x.workspace))
        .map((x) => x.id),
    ];
    const handler = (e: KeyboardEvent) => {
      const is = (id: string) => matches(e, byId[id] ?? '', apple);
      const s = useApp.getState();
      let handled = true;
      if (is('palette')) s.setPaletteOpen(!s.paletteOpen);
      else if (is('newTab')) s.setNewTabOpen(true);
      else if (is('jump')) s.setPaletteOpen(true);
      else if (is('local') && platform.localTerminal) sessions.openLocal();
      else if (is('closeTab') && s.activeTab !== VAULT_TAB && s.activeTab !== SFTP_TAB)
        s.closeTab(s.activeTab);
      else if (is('sidePanel')) s.setSidePanelOpen(!s.sidePanelOpen);
      else if (is('lock')) onLock();
      else if (is('settings')) s.setSection('settings');
      else if (is('broadcast')) {
        const ws = s.tabs.find((x) => x.id === s.activeTab);
        if (ws?.kind === 'workspace') s.updateTab(ws.id, { broadcast: !ws.broadcast });
      } else if (is('splitMode')) {
        const ws = s.tabs.find((x) => x.id === s.activeTab);
        if (ws?.kind === 'workspace')
          s.updateTab(ws.id, { mode: ws.mode === 'split' ? 'focus' : 'split' });
      } else if (is('nextTab') || is('prevTab')) {
        const ids = visible();
        const i = ids.indexOf(s.activeTab);
        const next = ids[(i + (is('nextTab') ? 1 : ids.length - 1)) % ids.length];
        if (next) s.activate(next);
      } else if ((apple ? e.metaKey : e.altKey) && /^[1-9]$/.test(e.key)) {
        const ids = visible();
        const target = ids[Number(e.key) - 1];
        if (target) s.activate(target);
      } else handled = false;
      if (handled) {
        e.preventDefault();
        e.stopPropagation();
      }
    };
    window.addEventListener('keydown', handler, { capture: true });
    return () => window.removeEventListener('keydown', handler, { capture: true });
  }, [platform, app.paletteOpen, sessions, onLock]);
}

function SectionView({
  status,
  onStatus,
  onImport,
}: {
  status: VaultStatus;
  onStatus: (s: VaultStatus) => void;
  onImport: () => void;
}) {
  const section = useApp((s) => s.section);
  switch (section) {
    case 'hosts':
      return <HostsView onImport={onImport} />;
    case 'keychain':
      return <KeychainView />;
    case 'forwarding':
      return <ForwardingView />;
    case 'snippets':
      return <SnippetsView />;
    case 'knownHosts':
      return <KnownHostsView onImport={onImport} />;
    case 'logs':
      return <LogsView />;
    case 'settings':
      return <SettingsView status={status} onStatus={onStatus} />;
  }
}

function Inspector() {
  const selected = useApp((s) => s.selected);
  const hosts = useHosts().data ?? EMPTY;
  const groups = useGroups().data ?? EMPTY;
  if (!selected) return null;
  if (selected.kind === 'host') {
    const h = hosts.find((x) => x.id === selected.id);
    return h ? <HostInspector key={h.id} host={h} /> : null;
  }
  const g = groups.find((x) => x.id === selected.id);
  return g ? <GroupInspector key={g.id} group={g} /> : null;
}

/** All session tabs stay mounted (hidden when inactive) so terminals keep their state. */
function SessionArea({ tabs, activeTab }: { tabs: Tab[]; activeTab: string }) {
  const sidePanelOpen = useApp((s) => s.sidePanelOpen);
  const bp = useBreakpoint();
  return (
    <>
      {tabs.map((tab) => {
        if (tab.kind === 'terminal' && tab.workspace) return null;
        const active = tab.id === activeTab;
        return (
          <div
            key={tab.id}
            className={cn('absolute inset-0 flex min-h-0', !active && 'invisible')}
            aria-hidden={!active}
          >
            <div className="min-w-0 flex-1">
              {tab.kind === 'terminal' ? <TerminalView tab={tab} active={active} /> : null}
              {tab.kind === 'workspace' ? <WorkspaceView ws={tab} active={active} /> : null}
              {tab.kind === 'log' ? <LogViewer tab={tab} /> : null}
            </div>
            {active &&
            sidePanelOpen &&
            bp === 'desktop' &&
            (tab.kind === 'terminal' || tab.kind === 'workspace') ? (
              <TerminalSidePanel
                tabId={tab.kind === 'workspace' ? (tab.focused ?? tab.panes[0] ?? tab.id) : tab.id}
                hostId={tab.kind === 'terminal' ? tab.hostId : null}
              />
            ) : null}
          </div>
        );
      })}
    </>
  );
}

function DesktopShell({ status, onStatus, onLock }: ShellProps) {
  const bp = useBreakpoint();
  const { activeTab, tabs, selected, select } = useApp();
  const [importOpen, setImportOpen] = useState(false);
  const inVault = activeTab === VAULT_TAB;
  const section = useApp((s) => s.section);
  const showInspector = inVault && selected && section === 'hosts';
  return (
    <div className="flex h-full flex-col">
      <TitleBar />
      <main className="relative min-h-0 flex-1">
        <div
          className={cn('absolute inset-0 flex', !inVault && 'invisible')}
          aria-hidden={!inVault}
        >
          <VaultNav compact={bp === 'tablet'} vaultName={status.active ?? ''} onLock={onLock} />
          <div className="min-w-0 flex-1">
            <SectionView status={status} onStatus={onStatus} onImport={() => setImportOpen(true)} />
          </div>
          {showInspector && bp === 'desktop' ? (
            <aside className="border-border bg-surface-1 animate-slide-in-right w-[380px] shrink-0 border-l">
              <Inspector />
            </aside>
          ) : null}
        </div>
        <div
          className={cn('absolute inset-0', activeTab !== SFTP_TAB && 'invisible')}
          aria-hidden={activeTab !== SFTP_TAB}
        >
          <SftpView />
        </div>
        <SessionArea tabs={tabs} activeTab={activeTab} />
      </main>
      {showInspector && bp !== 'desktop' ? (
        <Sheet open onOpenChange={(o) => !o && select(null)} title="">
          <Inspector />
        </Sheet>
      ) : null}
      <CommandPalette onLock={onLock} onImport={() => setImportOpen(true)} />
      <NewTabDialog />
      <ImportDialog open={importOpen} onOpenChange={setImportOpen} />
    </div>
  );
}

type PhoneTab = 'vault' | 'connections' | 'settings';

function PhoneShell({ status, onStatus, onLock }: ShellProps) {
  const { t } = useTranslation();
  const {
    tabs,
    activeTab,
    activate,
    closeTab,
    section,
    setSection,
    selected,
    select,
    setNewTabOpen,
  } = useApp();
  const hosts = useHosts().data ?? EMPTY;
  const [importOpen, setImportOpen] = useState(false);
  const [selectedPhoneTab, setPhoneTab] = useState<PhoneTab>('vault');
  const sessionTabs = useMemo(
    () => tabs.filter((x) => !(x.kind === 'terminal' && x.workspace)),
    [tabs],
  );
  const inSession =
    activeTab !== VAULT_TAB &&
    activeTab !== SFTP_TAB &&
    sessionTabs.some((x) => x.id === activeTab);

  // Opening Settings from elsewhere (palette, shortcuts) switches to the Settings tab.
  const phoneTab: PhoneTab = section === 'settings' ? 'settings' : selectedPhoneTab;

  return (
    <div className="flex h-full flex-col">
      {inSession ? (
        <div className="safe-top border-border bg-surface-1 flex shrink-0 [scrollbar-width:none] items-center gap-1.5 overflow-x-auto border-b px-2 py-1.5">
          <Button
            size="icon-sm"
            variant="ghost"
            aria-label={t('common.back')}
            onClick={() => {
              activate(VAULT_TAB);
              setPhoneTab('connections');
            }}
          >
            <ArrowLeft className="size-4" />
          </Button>
          {sessionTabs.map((tab) => (
            <button
              key={tab.id}
              type="button"
              onClick={() => activate(tab.id)}
              className={cn(
                'flex h-8 shrink-0 items-center gap-1.5 rounded-full pr-1.5 pl-3 text-[12px] font-medium',
                tab.id === activeTab ? 'bg-accent text-accent-fg' : 'bg-surface-3 text-muted',
              )}
            >
              {tab.title}
              <X
                className="size-3.5"
                aria-label={t('tabs.close', { title: tab.title })}
                onClick={(e) => {
                  e.stopPropagation();
                  closeTab(tab.id);
                }}
              />
            </button>
          ))}
          <Button
            size="icon-sm"
            variant="ghost"
            aria-label={t('tabs.new')}
            onClick={() => setNewTabOpen(true)}
          >
            <Plus className="size-4" />
          </Button>
        </div>
      ) : null}
      <main className={cn('relative min-h-0 flex-1', !inSession && 'safe-top')}>
        {!inSession ? (
          <div className="absolute inset-0 flex flex-col">
            {phoneTab === 'vault' ? (
              <>
                <div className="flex [scrollbar-width:none] gap-1 overflow-x-auto px-3 pt-3">
                  {SECTIONS.map((s) => (
                    <button
                      key={s.id}
                      type="button"
                      onClick={() => setSection(s.id)}
                      className={cn(
                        'flex h-8 shrink-0 items-center gap-1.5 rounded-full px-3 text-[12px] font-medium [&>svg]:size-3.5',
                        section === s.id ? 'bg-surface-3 text-text' : 'text-muted',
                      )}
                    >
                      {s.icon}
                      {t(s.key)}
                    </button>
                  ))}
                </div>
                <div className="min-h-0 flex-1">
                  {section === 'settings' ? (
                    <HostsView onImport={() => setImportOpen(true)} />
                  ) : (
                    <SectionView
                      status={status}
                      onStatus={onStatus}
                      onImport={() => setImportOpen(true)}
                    />
                  )}
                </div>
              </>
            ) : phoneTab === 'connections' ? (
              <div className="flex h-full flex-col overflow-y-auto px-4 pt-4">
                <h1 className="mb-3 text-lg font-semibold">{t('nav.connections')}</h1>
                {sessionTabs.length === 0 ? (
                  <EmptyState
                    icon={<SquareTerminal />}
                    title={t('nav.noConnections')}
                    description={t('nav.noConnectionsHint')}
                  />
                ) : (
                  <div className="grid gap-2">
                    {sessionTabs.map((tab) => {
                      const host =
                        tab.kind === 'terminal'
                          ? hosts.find((h) => h.id === tab.hostId)
                          : undefined;
                      return (
                        <button
                          key={tab.id}
                          type="button"
                          onClick={() => activate(tab.id)}
                          className="bg-surface-2 flex items-center gap-3 rounded-[var(--radius-card)] p-3 text-left"
                        >
                          {tab.kind === 'workspace' ? (
                            <LayoutGrid className="text-muted size-5" />
                          ) : (
                            <OsIcon os={host?.os} />
                          )}
                          <div className="min-w-0 flex-1">
                            <div className="truncate font-medium">{tab.title}</div>
                            <div className="text-subtle text-xs">
                              {tab.kind === 'terminal' ? t(`status.${tab.status}`) : tab.kind}
                            </div>
                          </div>
                          <X
                            className="text-subtle size-4"
                            aria-label={t('tabs.close', { title: tab.title })}
                            onClick={(e) => {
                              e.stopPropagation();
                              closeTab(tab.id);
                            }}
                          />
                        </button>
                      );
                    })}
                  </div>
                )}
                <div className="mt-6">
                  <SftpEntry />
                </div>
              </div>
            ) : (
              <SettingsView status={status} onStatus={onStatus} />
            )}
          </div>
        ) : null}
        <SessionArea tabs={sessionTabs} activeTab={inSession ? activeTab : ''} />
        {activeTab === SFTP_TAB ? (
          <div className="bg-bg absolute inset-0 flex flex-col">
            <Button variant="ghost" className="m-2 self-start" onClick={() => activate(VAULT_TAB)}>
              <ArrowLeft className="size-4" />
              {t('common.back')}
            </Button>
            <div className="min-h-0 flex-1">
              <SftpView />
            </div>
          </div>
        ) : null}
      </main>
      {!inSession && activeTab !== SFTP_TAB ? (
        <nav
          aria-label={t('nav.label')}
          className="border-border bg-surface-1 safe-bottom grid shrink-0 grid-cols-3 border-t"
        >
          {(
            [
              ['vault', <Server key="v" />, t('tabs.vault')],
              ['connections', <SquareTerminal key="c" />, t('nav.connections')],
              ['settings', <Settings key="s" />, t('nav.settings')],
            ] as const
          ).map(([id, icon, label]) => (
            <button
              key={id}
              type="button"
              aria-current={phoneTab === id ? 'page' : undefined}
              onClick={() => {
                setPhoneTab(id);
                if (id !== 'settings' && section === 'settings') setSection('hosts');
                if (id === 'settings') setSection('settings');
              }}
              className={cn(
                'flex h-14 flex-col items-center justify-center gap-0.5 text-[11px] font-medium [&>svg]:size-5',
                phoneTab === id ? 'text-accent' : 'text-muted',
              )}
            >
              {icon}
              {label}
              {id === 'connections' && sessionTabs.length > 0 ? (
                <span className="sr-only">({sessionTabs.length})</span>
              ) : null}
            </button>
          ))}
        </nav>
      ) : null}
      {selected ? (
        <Sheet open onOpenChange={(o) => !o && select(null)} title="">
          <Inspector />
        </Sheet>
      ) : null}
      <CommandPalette onLock={onLock} onImport={() => setImportOpen(true)} />
      <NewTabDialog />
      <ImportDialog open={importOpen} onOpenChange={setImportOpen} />
    </div>
  );
}

function SftpEntry() {
  const { t } = useTranslation();
  const activate = useApp((s) => s.activate);
  return (
    <Button className="w-full" onClick={() => activate(SFTP_TAB)}>
      {t('tabs.sftp')}
    </Button>
  );
}

export function Shell(props: ShellProps) {
  const bp = useBreakpoint();
  useShortcuts(props.onLock);
  return bp === 'phone' ? <PhoneShell {...props} /> : <DesktopShell {...props} />;
}
