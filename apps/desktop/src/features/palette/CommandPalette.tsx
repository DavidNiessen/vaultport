// SPDX-License-Identifier: GPL-3.0-or-later
/** ⌘K / Ctrl+K command palette: hosts, open tabs, snippets and app actions. */
import { Command } from 'cmdk';
import {
  Code2,
  KeyRound,
  Lock,
  Moon,
  Plus,
  Settings,
  SquareTerminal,
  Sun,
  Upload,
} from 'lucide-react';
import type { ReactNode } from 'react';
import { useTranslation } from 'react-i18next';

import { OsIcon } from '@/components/OsIcon';
import { Dialog } from '@/components/ui/overlay';
import { useHosts, useSnippets } from '@/lib/queries';
import { EMPTY } from '@/lib/empty';
import { usePlatform } from '@/lib/platform';
import { VAULT_TAB, useApp } from '@/stores/app';

import { useSessions } from '../terminal/sessions';
import { sendInput } from '../terminal/registry';

const itemCls =
  'flex h-10 items-center gap-3 rounded-lg px-3 text-[13px] data-[selected=true]:bg-surface-3 [&>svg]:size-4 [&>svg]:text-muted';

export function CommandPalette({ onLock, onImport }: { onLock: () => void; onImport: () => void }) {
  const { t } = useTranslation();
  const platform = usePlatform();
  const {
    paletteOpen,
    setPaletteOpen,
    tabs,
    activeTab,
    activate,
    setSection,
    theme,
    setTheme,
    select,
  } = useApp();
  const hosts = useHosts().data ?? EMPTY;
  const snippets = useSnippets().data ?? EMPTY;
  const sessions = useSessions();
  const close = () => setPaletteOpen(false);
  const run = (fn: () => void) => () => {
    close();
    fn();
  };
  const activeTerminal = tabs.find((x) => x.id === activeTab && x.kind === 'terminal');

  const action = (label: string, icon: ReactNode, fn: () => void, keywords?: string[]) => (
    <Command.Item value={label} keywords={keywords} onSelect={run(fn)} className={itemCls}>
      {icon}
      {label}
    </Command.Item>
  );

  return (
    <Dialog
      open={paletteOpen}
      onOpenChange={setPaletteOpen}
      title={t('palette.title')}
      className="sm:top-[18%] sm:translate-y-0"
    >
      <Command label={t('palette.title')} className="-mx-2" loop>
        <Command.Input
          autoFocus
          placeholder={t('palette.placeholder')}
          className="bg-surface-3 text-text placeholder:text-subtle mx-2 mb-2 h-10 w-[calc(100%-1rem)] rounded-xl px-3 text-sm outline-none"
        />
        <Command.List className="max-h-[50vh] overflow-y-auto px-2">
          <Command.Empty className="text-subtle py-6 text-center text-[13px]">
            {t('palette.empty')}
          </Command.Empty>
          {tabs.length > 0 ? (
            <Command.Group
              heading={t('palette.tabs')}
              className="text-subtle [&_[cmdk-group-heading]]:px-3 [&_[cmdk-group-heading]]:py-1.5 [&_[cmdk-group-heading]]:text-[11px] [&_[cmdk-group-heading]]:font-semibold [&_[cmdk-group-heading]]:uppercase"
            >
              {tabs.map((tab) => (
                <Command.Item
                  key={tab.id}
                  value={`tab ${tab.title} ${tab.id}`}
                  onSelect={run(() => activate(tab.id))}
                  className={itemCls}
                >
                  <SquareTerminal />
                  {tab.title}
                </Command.Item>
              ))}
            </Command.Group>
          ) : null}
          <Command.Group
            heading={t('palette.hosts')}
            className="text-subtle [&_[cmdk-group-heading]]:px-3 [&_[cmdk-group-heading]]:py-1.5 [&_[cmdk-group-heading]]:text-[11px] [&_[cmdk-group-heading]]:font-semibold [&_[cmdk-group-heading]]:uppercase"
          >
            {hosts.map((h) => (
              <Command.Item
                key={h.id}
                value={`host ${h.label} ${h.address} ${h.tags.join(' ')} ${h.id}`}
                onSelect={run(() => sessions.connectHost(h))}
                className={itemCls}
              >
                <OsIcon os={h.os} size="sm" />
                <span className="text-text flex-1 truncate">{h.label}</span>
                <span className="text-subtle truncate text-xs">{h.address}</span>
              </Command.Item>
            ))}
          </Command.Group>
          {activeTerminal && snippets.length > 0 ? (
            <Command.Group
              heading={t('palette.snippets')}
              className="text-subtle [&_[cmdk-group-heading]]:px-3 [&_[cmdk-group-heading]]:py-1.5 [&_[cmdk-group-heading]]:text-[11px] [&_[cmdk-group-heading]]:font-semibold [&_[cmdk-group-heading]]:uppercase"
            >
              {snippets.map((s) => (
                <Command.Item
                  key={s.id}
                  value={`snippet ${s.label} ${s.script} ${s.id}`}
                  onSelect={run(() => sendInput([activeTerminal.id], `${s.script}\r`))}
                  className={itemCls}
                >
                  <Code2 />
                  <span className="text-text">{s.label}</span>
                </Command.Item>
              ))}
            </Command.Group>
          ) : null}
          <Command.Group
            heading={t('palette.actions')}
            className="text-subtle [&_[cmdk-group-heading]]:px-3 [&_[cmdk-group-heading]]:py-1.5 [&_[cmdk-group-heading]]:text-[11px] [&_[cmdk-group-heading]]:font-semibold [&_[cmdk-group-heading]]:uppercase"
          >
            {action(t('hosts.newHost'), <Plus />, () => {
              activate(VAULT_TAB);
              setSection('hosts');
              select(null);
              window.dispatchEvent(new CustomEvent('vaultport:new-host'));
            })}
            {platform.localTerminal
              ? action(t('hosts.terminal'), <SquareTerminal />, () => sessions.openLocal(), [
                  'local',
                  'shell',
                ])
              : null}
            {action(t('nav.keychain'), <KeyRound />, () => setSection('keychain'))}
            {action(t('nav.snippets'), <Code2 />, () => setSection('snippets'))}
            {action(t('hosts.import'), <Upload />, onImport)}
            {action(t('nav.settings'), <Settings />, () => setSection('settings'))}
            {action(
              theme === 'light' ? t('palette.darkTheme') : t('palette.lightTheme'),
              theme === 'light' ? <Moon /> : <Sun />,
              () => setTheme(theme === 'light' ? 'dark' : 'light'),
            )}
            {action(t('lock.lockNow'), <Lock />, onLock)}
          </Command.Group>
        </Command.List>
      </Command>
    </Dialog>
  );
}
