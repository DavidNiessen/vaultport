// SPDX-License-Identifier: GPL-3.0-or-later
/** ⌘K / Ctrl+K command palette: hosts, open tabs, snippets and app actions. */
import { Command } from 'cmdk';
import {
  ArrowDown,
  ArrowUp,
  Code2,
  Copy,
  Eraser,
  History,
  KeyRound,
  Lock,
  Moon,
  Pencil,
  Plug,
  Plus,
  Search,
  Settings,
  Sparkles,
  SquarePen,
  SquareTerminal,
  Sun,
  Type,
  Undo2,
  Upload,
  Wand2,
  ZoomIn,
  ZoomOut,
} from 'lucide-react';
import type { ReactNode } from 'react';
import { useTranslation } from 'react-i18next';

import { OsIcon } from '@/components/OsIcon';
import { Kbd } from '@/components/ui/controls';
import { Dialog } from '@/components/ui/overlay';
import { useHosts, useSnippets } from '@/lib/queries';
import { EMPTY } from '@/lib/empty';
import { formatShortcut, usePlatform } from '@/lib/platform';
import { VAULT_TAB, useApp } from '@/stores/app';

import { useSessions } from '../terminal/sessions';
import { sendInput, terminalRegistry, type TerminalAction } from '../terminal/registry';
import { activeTerminal as activeTerminalTab, duplicate } from '../terminal/tabActions';
import { primaryBinding, useKeymap, type ShortcutId } from './shortcuts';

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
    setNewTabOpen,
    setRenaming,
    closedTabs,
    reopenClosed,
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
  const keymap = useKeymap((st) => st.keymap);
  const current = tabs.find((x) => x.id === activeTab);

  const action = (
    label: string,
    icon: ReactNode,
    fn: () => void,
    keywords?: string[],
    shortcut?: ShortcutId,
  ) => {
    const keys = shortcut ? primaryBinding(keymap, shortcut) : undefined;
    return (
      <Command.Item
        key={label}
        value={label}
        keywords={keywords}
        onSelect={run(fn)}
        className={itemCls}
      >
        {icon}
        <span className="flex-1">{label}</span>
        {keys ? <Kbd>{formatShortcut(platform, keys)}</Kbd> : null}
      </Command.Item>
    );
  };
  const term = activeTerminalTab();
  const termAction = (id: TerminalAction & ShortcutId, icon: ReactNode, keywords?: string[]) =>
    term
      ? action(
          t(`shortcuts.${id}`),
          icon,
          () => {
            const h = terminalRegistry.get(term.id);
            h?.focus();
            // Let the palette close and the terminal take focus first.
            setTimeout(() => h?.action(id), 0);
          },
          keywords,
          id,
        )
      : null;

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
          {term ? (
            <Command.Group
              heading={t('palette.terminal')}
              className="text-subtle [&_[cmdk-group-heading]]:px-3 [&_[cmdk-group-heading]]:py-1.5 [&_[cmdk-group-heading]]:text-[11px] [&_[cmdk-group-heading]]:font-semibold [&_[cmdk-group-heading]]:uppercase"
            >
              {termAction('historySearch', <History />, ['history', 'reverse'])}
              {termAction('aiCommand', <Sparkles />, ['ai', 'llm', 'natural language'])}
              {termAction('composer', <SquarePen />, ['multi-line', 'editor'])}
              {termAction('search', <Search />, ['find'])}
              {termAction('clear', <Eraser />, ['cls', 'reset'])}
              {termAction('prevBlock', <ArrowUp />, ['block', 'jump'])}
              {termAction('nextBlock', <ArrowDown />, ['block', 'jump'])}
              {termAction('copyLastOutput', <Copy />, ['block', 'clipboard'])}
              {term.protocol === 'ssh' || term.protocol === 'mosh'
                ? action(
                    t('terminal.installIntegration'),
                    <Wand2 />,
                    () => {
                      const h = terminalRegistry.get(term.id);
                      h?.focus();
                      setTimeout(() => h?.action('installIntegration'), 0);
                    },
                    ['shell integration', 'blocks', 'warpify'],
                  )
                : null}
              {termAction('zoomIn', <ZoomIn />, ['font', 'bigger'])}
              {termAction('zoomOut', <ZoomOut />, ['font', 'smaller'])}
              {termAction('zoomReset', <Type />, ['font'])}
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
              ? action(
                  t('shortcuts.newTab'),
                  <SquareTerminal />,
                  () => sessions.openLocal(),
                  ['local', 'shell'],
                  'newTab',
                )
              : null}
            {action(
              t('shortcuts.connect'),
              <Plug />,
              () => setNewTabOpen(true),
              ['ssh', 'host'],
              'connect',
            )}
            {current && current.kind !== 'sftp'
              ? action(
                  t('shortcuts.renameTab'),
                  <Pencil />,
                  () => setRenaming(current.id),
                  [],
                  'renameTab',
                )
              : null}
            {current?.kind === 'terminal'
              ? action(
                  t('shortcuts.duplicateTab'),
                  <Copy />,
                  () => duplicate(current.id),
                  [],
                  'duplicateTab',
                )
              : null}
            {closedTabs.length > 0
              ? action(
                  t('shortcuts.reopenTab'),
                  <Undo2 />,
                  () => reopenClosed(),
                  ['restore', 'undo'],
                  'reopenTab',
                )
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
