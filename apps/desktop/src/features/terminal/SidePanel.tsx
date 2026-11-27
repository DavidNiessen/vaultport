// SPDX-License-Identifier: GPL-3.0-or-later
/** Terminal side panel: snippets, command history, themes and the local AI prompt. */
import { useQuery } from '@tanstack/react-query';
import { Code2, History, Palette, Play, Sparkles, X } from 'lucide-react';
import { useState, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';

import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/controls';
import { Input, Textarea } from '@/components/ui/input';
import { cn } from '@/lib/cn';
import { EMPTY } from '@/lib/empty';
import { commands, errorMessage } from '@/lib/ipc';
import { useHosts, useSaveHost, useSettings, useSnippets } from '@/lib/queries';
import { useApp } from '@/stores/app';

import { DEFAULT_AI_SETTINGS, type AiSettings } from '../ai/settings';
import { renderSnippet, snippetVariables } from '../snippets/SnippetsView';
import { sendInput, terminalRegistry } from './registry';
import { TERMINAL_THEMES, findTheme, themePreviewBackground } from './themes';

type Panel = 'snippets' | 'history' | 'themes' | 'ai';

export function TerminalSidePanel({ tabId, hostId }: { tabId: string; hostId: string | null }) {
  const { t } = useTranslation();
  const [panel, setPanel] = useState<Panel>('snippets');
  const setSidePanelOpen = useApp((s) => s.setSidePanelOpen);
  const { settings: ai } = useSettings<AiSettings>('ai', DEFAULT_AI_SETTINGS);
  const tabs: { id: Panel; icon: ReactNode; label: string }[] = [
    { id: 'snippets', icon: <Code2 />, label: t('nav.snippets') },
    { id: 'history', icon: <History />, label: t('terminal.history') },
    { id: 'themes', icon: <Palette />, label: t('terminal.themes') },
    ...(ai.enabled ? [{ id: 'ai' as const, icon: <Sparkles />, label: t('ai.short') }] : []),
  ];
  return (
    <aside
      aria-label={t('terminal.sidePanel')}
      className="border-border bg-surface-1 flex w-80 shrink-0 flex-col border-l"
    >
      <div className="flex items-center gap-1 p-2">
        {tabs.map((x) => (
          <button
            key={x.id}
            type="button"
            aria-label={x.label}
            aria-pressed={panel === x.id}
            title={x.label}
            onClick={() => setPanel(x.id)}
            className={cn(
              'flex size-8 items-center justify-center rounded-lg [&>svg]:size-4',
              panel === x.id ? 'bg-accent-soft text-accent-text' : 'text-muted hover:bg-surface-2',
            )}
          >
            {x.icon}
          </button>
        ))}
        <div className="flex-1" />
        <Button
          size="icon-sm"
          variant="ghost"
          aria-label={t('common.close')}
          onClick={() => setSidePanelOpen(false)}
        >
          <X className="size-4" />
        </Button>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto px-3 pb-3">
        {panel === 'snippets' && <SnippetList tabId={tabId} />}
        {panel === 'history' && <HistoryList tabId={tabId} hostId={hostId} />}
        {panel === 'themes' && <ThemeList hostId={hostId} />}
        {panel === 'ai' && <AiPrompt tabId={tabId} />}
      </div>
    </aside>
  );
}

function SnippetList({ tabId }: { tabId: string }) {
  const { t } = useTranslation();
  const snippets = useSnippets().data ?? EMPTY;
  const tabs = useApp((s) => s.tabs);
  const [q, setQ] = useState('');
  const run = (script: string, execute: boolean, all: boolean) => {
    if (snippetVariables(script).some((v) => !v.default)) {
      toast.error(t('snippets.needsVariables'));
      return;
    }
    const text = renderSnippet(script, {}) + (execute ? '\r' : '');
    const targets = all
      ? tabs.filter((x) => x.kind === 'terminal' && x.status === 'connected').map((x) => x.id)
      : [tabId];
    sendInput(targets, text);
    terminalRegistry.get(tabId)?.focus();
  };
  return (
    <div className="grid gap-2">
      <Input
        aria-label={t('common.search')}
        placeholder={t('snippets.search')}
        value={q}
        onChange={(e) => setQ(e.target.value)}
      />
      {snippets
        .filter((s) => !q || `${s.label} ${s.script}`.toLowerCase().includes(q.toLowerCase()))
        .map((s) => (
          <div
            key={s.id}
            className="bg-surface-2 group rounded-xl p-2.5"
            onDoubleClick={() => run(s.script, true, false)}
          >
            <div className="text-accent-text truncate text-[13px] font-medium">{s.label}</div>
            <pre className="text-muted line-clamp-2 font-mono text-[11px] whitespace-pre-wrap">
              {s.script}
            </pre>
            <div className="mt-1.5 flex gap-1">
              <Button size="sm" variant="primary" onClick={() => run(s.script, true, false)}>
                <Play className="size-3" />
                {t('snippets.run')}
              </Button>
              <Button size="sm" variant="ghost" onClick={() => run(s.script, false, false)}>
                {t('snippets.paste')}
              </Button>
              <Button size="sm" variant="ghost" onClick={() => run(s.script, true, true)}>
                {t('snippets.runAll')}
              </Button>
            </div>
          </div>
        ))}
    </div>
  );
}

function HistoryList({ tabId, hostId }: { tabId: string; hostId: string | null }) {
  const { t } = useTranslation();
  const history =
    useQuery({ queryKey: ['history', hostId], queryFn: () => commands.historyList(hostId, 200) })
      .data ?? EMPTY;
  return (
    <div className="grid gap-1">
      {history.length === 0 ? (
        <p className="text-subtle py-4 text-center text-xs">{t('terminal.noHistory')}</p>
      ) : null}
      {history.map((h, i) => (
        <button
          key={`${h.command}-${String(i)}`}
          type="button"
          className="hover:bg-surface-2 rounded-lg px-2 py-1.5 text-left font-mono text-[12px]"
          onClick={() => sendInput([tabId], h.command)}
          onDoubleClick={() => sendInput([tabId], `${h.command}\r`)}
        >
          <span className="line-clamp-2">{h.command}</span>
          {h.count > 1 ? <Badge className="mt-0.5">{h.count}×</Badge> : null}
        </button>
      ))}
    </div>
  );
}

function ThemeList({ hostId }: { hostId: string | null }) {
  const { t } = useTranslation();
  const hosts = useHosts().data ?? EMPTY;
  const save = useSaveHost();
  const host = hosts.find((h) => h.id === hostId);
  const current = host?.config.terminal.theme ?? null;
  return (
    <div className="grid gap-2">
      {!host ? <p className="text-subtle text-xs">{t('terminal.themeQuickOnly')}</p> : null}
      {host ? (
        <div className="flex items-center gap-2">
          <span className="text-muted text-xs">{t('hosts.fontSize')}</span>
          {[-1, 1].map((d) => (
            <Button
              key={d}
              size="sm"
              variant="secondary"
              onClick={() =>
                save.mutate({
                  ...host,
                  config: {
                    ...host.config,
                    terminal: {
                      ...host.config.terminal,
                      fontSize: Math.min(
                        32,
                        Math.max(8, (host.config.terminal.fontSize ?? 14) + d),
                      ),
                    },
                  },
                })
              }
            >
              {d < 0 ? 'A−' : 'A+'}
            </Button>
          ))}
        </div>
      ) : null}
      <div className="grid grid-cols-2 gap-2">
        {TERMINAL_THEMES.map((th) => {
          const c = findTheme(th.id, 'dark').colors;
          return (
            <button
              key={th.id}
              type="button"
              disabled={!host}
              onClick={() =>
                host &&
                save.mutate({
                  ...host,
                  config: { ...host.config, terminal: { ...host.config.terminal, theme: th.id } },
                })
              }
              className={cn(
                'rounded-lg p-1 text-left ring-2 ring-transparent disabled:opacity-50',
                current === th.id && 'ring-accent',
              )}
            >
              <div
                className="flex h-9 items-center gap-1 rounded-md px-2"
                style={{ background: themePreviewBackground(findTheme(th.id, 'dark')) }}
              >
                {[c.red, c.green, c.yellow, c.blue, c.magenta].map((col, i) => (
                  <span
                    key={String(i)}
                    className="h-1.5 flex-1 rounded-full"
                    style={{ background: col }}
                  />
                ))}
              </div>
              <div className="truncate px-0.5 pt-1 text-[11px]">{th.name}</div>
            </button>
          );
        })}
      </div>
    </div>
  );
}

function AiPrompt({ tabId }: { tabId: string }) {
  const { t } = useTranslation();
  const [prompt, setPrompt] = useState('');
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<{
    command: string;
    explanation: string;
    risk: string;
  } | null>(null);
  const generate = async () => {
    setBusy(true);
    try {
      const selection = terminalRegistry.get(tabId)?.term.getSelection() ?? '';
      setResult(await commands.aiGenerateCommand(prompt, tabId, selection || null));
    } catch (e) {
      toast.error(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <div className="grid gap-2">
      <Textarea
        aria-label={t('ai.prompt')}
        rows={3}
        placeholder={t('ai.promptPlaceholder')}
        value={prompt}
        onChange={(e) => setPrompt(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === 'Enter' && !e.shiftKey) {
            e.preventDefault();
            void generate();
          }
        }}
      />
      <Button
        variant="primary"
        loading={busy}
        disabled={!prompt.trim()}
        onClick={() => void generate()}
      >
        <Sparkles className="size-4" />
        {t('ai.generate')}
      </Button>
      {result ? (
        <div className="bg-surface-2 grid gap-2 rounded-xl p-3">
          <code className="selectable font-mono text-[12px] break-all">{result.command}</code>
          <p className="text-muted text-xs">{result.explanation}</p>
          <Badge
            tone={
              result.risk === 'low' ? 'success' : result.risk === 'medium' ? 'warning' : 'danger'
            }
          >
            {t(`ai.risk.${result.risk}`)}
          </Badge>
          <Button size="sm" onClick={() => sendInput([tabId], result.command)}>
            {t('ai.insert')}
          </Button>
          <p className="text-subtle text-[11px]">{t('ai.neverAutoRun')}</p>
        </div>
      ) : null}
    </div>
  );
}
