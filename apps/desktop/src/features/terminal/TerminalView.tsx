// SPDX-License-Identifier: GPL-3.0-or-later
/**
 * A terminal pane: an xterm.js instance bound to a core session. Output arrives on a Tauri
 * channel (base64 chunks, batched by the core); keystrokes go back through `session_write`.
 */
import { Channel } from '@tauri-apps/api/core';
import { writeText } from '@tauri-apps/plugin-clipboard-manager';
import { openUrl } from '@tauri-apps/plugin-opener';
import { FitAddon } from '@xterm/addon-fit';
import { SearchAddon } from '@xterm/addon-search';
import { Unicode11Addon } from '@xterm/addon-unicode11';
import { WebLinksAddon } from '@xterm/addon-web-links';
import { WebglAddon } from '@xterm/addon-webgl';
import { Terminal } from '@xterm/xterm';
import { ChevronDown, ChevronUp, Loader2, RotateCw, X } from 'lucide-react';
import { useCallback, useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';

import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { cn } from '@/lib/cn';
import { EMPTY } from '@/lib/empty';
import { commands, errorMessage, isTauri, type SessionEvent } from '@/lib/ipc';
import { useBreakpoint, useColorScheme } from '@/lib/platform';
import { useHosts, useSettings } from '@/lib/queries';
import { useApp, type TerminalTab } from '@/stores/app';

import { AccessoryBar } from './AccessoryBar';
import { AuthPromptDialog, HostKeyDialog, type PendingPrompt } from './Prompts';
import { highlightKeywords } from './highlight';
import { DEFAULT_TERMINAL_SETTINGS, type TerminalSettings } from './settings';
import { findTheme } from './themes';
import { terminalRegistry } from './registry';

function decodeBase64(b64: string): Uint8Array {
  const bin = atob(b64);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}

export function TerminalView({ tab, active }: { tab: TerminalTab; active: boolean }) {
  const { t } = useTranslation();
  const containerRef = useRef<HTMLDivElement>(null);
  const termRef = useRef<Terminal | null>(null);
  const fitRef = useRef<FitAddon | null>(null);
  const searchRef = useRef<SearchAddon | null>(null);
  const sessionRef = useRef<string | null>(null);
  const [prompt, setPrompt] = useState<PendingPrompt | null>(null);
  const [searchOpen, setSearchOpen] = useState(false);
  const [searchTerm, setSearchTerm] = useState('');
  const [attempt, setAttempt] = useState(0);
  const [closedReason, setClosedReason] = useState<string | null>(null);
  const updateTab = useApp((s) => s.updateTab);
  const theme = useApp((s) => s.theme);
  const scheme = useColorScheme(theme);
  const bp = useBreakpoint();
  const hosts = useHosts().data ?? EMPTY;
  const host = hosts.find((h) => h.id === tab.hostId);
  const { settings } = useSettings<TerminalSettings>('terminal', DEFAULT_TERMINAL_SETTINGS);
  const settingsRef = useRef(settings);
  useEffect(() => {
    settingsRef.current = settings;
  }, [settings]);

  const themeId = host?.config.terminal.theme ?? settings.theme;
  const fontSize = host?.config.terminal.fontSize ?? settings.fontSize;

  // Create the terminal once.
  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    const term = new Terminal({
      allowProposedApi: true,
      fontFamily: `'${settingsRef.current.fontFamily}', 'JetBrains Mono', monospace`,
      fontSize,
      lineHeight: 1.15,
      cursorBlink: settingsRef.current.cursorBlink,
      cursorStyle: settingsRef.current.cursorStyle,
      scrollback: settingsRef.current.scrollback,
      drawBoldTextInBrightColors: settingsRef.current.brightBold,
      macOptionIsMeta: settingsRef.current.optionAsMeta,
      rightClickSelectsWord: false,
      theme: findTheme(themeId, scheme).colors,
      linkHandler: {
        activate: (_e, uri) => {
          void confirmAndOpen(uri);
        },
        allowNonHttpProtocols: false,
      },
    });
    const fit = new FitAddon();
    const search = new SearchAddon();
    term.loadAddon(fit);
    term.loadAddon(search);
    term.loadAddon(new Unicode11Addon());
    term.unicode.activeVersion = '11';
    term.loadAddon(
      new WebLinksAddon((event, uri) => {
        // Require Ctrl/Cmd+click to avoid accidental navigation.
        if (event.ctrlKey || event.metaKey) void confirmAndOpen(uri);
      }),
    );
    term.open(el);
    try {
      const webgl = new WebglAddon();
      webgl.onContextLoss(() => {
        webgl.dispose();
      });
      term.loadAddon(webgl);
    } catch {
      // Canvas/DOM renderer fallback (e.g. WebKitGTK without WebGL).
    }

    // OSC 52 clipboard writes, gated by user policy. Clipboard *reads* are never allowed.
    term.parser.registerOscHandler(52, (data) => {
      const [, payload] = data.split(';', 2);
      if (!payload || payload === '?') return true;
      const policy = settingsRef.current.osc52;
      if (policy === 'deny') return true;
      let text: string;
      try {
        text = new TextDecoder().decode(decodeBase64(payload));
      } catch {
        return true;
      }
      const write = () => {
        void writeText(text).then(() => toast.success(t('terminal.copiedByRemote')));
      };
      if (policy === 'allow') write();
      else
        toast(t('terminal.osc52Request', { count: text.length }), {
          action: { label: t('common.allow'), onClick: write },
        });
      return true;
    });

    term.onSelectionChange(() => {
      if (settingsRef.current.copyOnSelect && term.hasSelection()) {
        void writeText(term.getSelection());
      }
    });
    term.onData((data) => {
      // Broadcast input to every pane of the workspace when enabled.
      const ws = useApp
        .getState()
        .tabs.find((x) => x.kind === 'workspace' && x.panes.includes(tab.id));
      if (ws?.kind === 'workspace' && ws.broadcast) {
        for (const pane of ws.panes) {
          const sid =
            pane === tab.id
              ? sessionRef.current
              : (terminalRegistry.get(pane)?.sessionId() ?? null);
          if (sid) void commands.sessionWrite(sid, data).catch(() => undefined);
        }
        return;
      }
      const id = sessionRef.current;
      if (id) void commands.sessionWrite(id, data).catch(() => undefined);
    });
    term.onBinary((data) => {
      const id = sessionRef.current;
      if (id) void commands.sessionWrite(id, data).catch(() => undefined);
    });
    term.onResize(({ cols, rows }) => {
      const id = sessionRef.current;
      if (id) void commands.sessionResize(id, cols, rows).catch(() => undefined);
    });
    term.onTitleChange((title) => {
      if (title && settingsRef.current.remoteTitles) updateTab(tab.id, { title });
    });
    term.attachCustomKeyEventHandler((e) => {
      if (e.type !== 'keydown') return true;
      const mod = e.ctrlKey || e.metaKey;
      if (mod && e.shiftKey && e.key.toLowerCase() === 'f') {
        setSearchOpen(true);
        return false;
      }
      if (mod && e.shiftKey && e.key.toLowerCase() === 'c' && term.hasSelection()) {
        void writeText(term.getSelection());
        return false;
      }
      if (e.metaKey && e.key.toLowerCase() === 'c' && term.hasSelection()) {
        void writeText(term.getSelection());
        return false;
      }
      return true;
    });

    termRef.current = term;
    fitRef.current = fit;
    searchRef.current = search;
    terminalRegistry.set(tab.id, {
      term,
      write: (data) => {
        const id = sessionRef.current;
        if (id) void commands.sessionWrite(id, data);
      },
      focus: () => term.focus(),
      sessionId: () => sessionRef.current,
    });
    const ro = new ResizeObserver(() => {
      if (el.offsetParent !== null) fit.fit();
    });
    ro.observe(el);
    requestAnimationFrame(() => fit.fit());
    return () => {
      ro.disconnect();
      terminalRegistry.delete(tab.id);
      term.dispose();
      termRef.current = null;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps -- created once per tab
  }, [tab.id]);

  // Live theme / font updates.
  useEffect(() => {
    const term = termRef.current;
    if (!term) return;
    term.options.theme = findTheme(themeId, scheme).colors;
    term.options.fontSize = fontSize;
    term.options.fontFamily = `'${settings.fontFamily}', 'JetBrains Mono', monospace`;
    term.options.cursorBlink = settings.cursorBlink;
    term.options.cursorStyle = settings.cursorStyle;
    fitRef.current?.fit();
  }, [themeId, scheme, fontSize, settings]);

  // Open (and re-open) the session.
  useEffect(() => {
    const term = termRef.current;
    if (!term) return;
    let disposed = false;
    setClosedReason(null);
    updateTab(tab.id, { status: 'connecting' });
    const events = new Channel<SessionEvent>();
    events.onmessage = (ev) => {
      if (disposed) return;
      switch (ev.type) {
        case 'output': {
          const bytes = decodeBase64(ev.data);
          term.write(bytes);
          if (settingsRef.current.highlightKeywords) highlightKeywords(term);
          if (!document.hasFocus() || useApp.getState().activeTab !== tab.id) {
            updateTab(tab.id, { unread: true });
          }
          break;
        }
        case 'status':
          if (ev.status === 'connected') {
            updateTab(tab.id, { status: 'connected' });
            const initial = useApp.getState().tabs.find((x) => x.id === tab.id);
            if (initial?.kind === 'terminal' && initial.initialInput) {
              const text = initial.initialInput;
              updateTab(tab.id, { initialInput: undefined });
              setTimeout(() => {
                const id = sessionRef.current;
                if (id) void commands.sessionWrite(id, text);
              }, 400);
            }
          }
          break;
        case 'banner':
          term.write(`\x1b[2m${ev.message.replace(/\n/g, '\r\n')}\x1b[0m\r\n`);
          break;
        case 'hostKey':
          setPrompt({ kind: 'hostKey', event: ev });
          break;
        case 'auth':
          setPrompt({ kind: 'auth', event: ev });
          break;
        case 'closed':
          sessionRef.current = null;
          updateTab(tab.id, { status: ev.error ? 'error' : 'disconnected', sessionId: null });
          setClosedReason(ev.reason ?? null);
          if (ev.error)
            term.write(`\r\n\x1b[31m${ev.reason ?? t('terminal.connectionFailed')}\x1b[0m\r\n`);
          else term.write(`\r\n\x1b[2m${t('terminal.sessionClosed')}\x1b[0m\r\n`);
          if (!ev.error && settingsRef.current.autoReconnect && ev.reason === 'network') {
            setTimeout(() => setAttempt((a) => a + 1), 1500);
          }
          break;
      }
    };
    const target =
      tab.protocol === 'serial' && tab.serial
        ? ({ type: 'serial', ...tab.serial } as const)
        : tab.protocol === 'local'
          ? ({ type: 'local' } as const)
          : tab.hostId
            ? ({ type: 'host', hostId: tab.hostId } as const)
            : ({
                type: 'quick',
                host: tab.quick?.host ?? '',
                port: tab.quick?.port ?? null,
                username: tab.quick?.username ?? null,
              } as const);
    const { cols, rows } = term;
    commands
      .sessionOpen(
        { target, cols, rows },
        events as unknown as Parameters<typeof commands.sessionOpen>[1],
      )
      .then((id) => {
        if (disposed) {
          void commands.sessionClose(id);
          return;
        }
        sessionRef.current = id;
        updateTab(tab.id, { sessionId: id });
      })
      .catch((e: unknown) => {
        if (disposed) return;
        updateTab(tab.id, { status: 'error' });
        setClosedReason(errorMessage(e));
        term.write(`\r\n\x1b[31m${errorMessage(e)}\x1b[0m\r\n`);
      });
    return () => {
      disposed = true;
      const id = sessionRef.current;
      sessionRef.current = null;
      if (id) void commands.sessionClose(id);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps -- reconnect via `attempt`
  }, [tab.id, attempt]);

  useEffect(() => {
    if (active) {
      requestAnimationFrame(() => {
        fitRef.current?.fit();
        termRef.current?.focus();
      });
    }
  }, [active]);

  const runSearch = useCallback(
    (dir: 'next' | 'prev') => {
      const s = searchRef.current;
      if (!s || !searchTerm) return;
      const opts = {
        caseSensitive: false,
        decorations: {
          matchOverviewRuler: '#888',
          activeMatchColorOverviewRuler: '#6366f1',
          matchBackground: '#6366f155',
          activeMatchBackground: '#6366f1',
        },
      };
      if (dir === 'next') s.findNext(searchTerm, opts);
      else s.findPrevious(searchTerm, opts);
    },
    [searchTerm],
  );

  const respond = (accept: boolean, extra?: { remember?: boolean; answers?: string[] }) => {
    const id = sessionRef.current;
    const p = prompt;
    setPrompt(null);
    if (!id || !p) return;
    const response =
      p.kind === 'hostKey'
        ? ({ type: 'hostKey', accept, remember: extra?.remember ?? true } as const)
        : ({ type: 'auth', answers: accept ? (extra?.answers ?? []) : null } as const);
    void commands
      .sessionRespond(id, p.event.promptId, response)
      .catch((e: unknown) => toast.error(errorMessage(e)));
  };

  const background = findTheme(themeId, scheme).colors.background;

  return (
    <div className="relative flex h-full min-h-0 flex-col" style={{ background }}>
      {searchOpen ? (
        <div className="bg-surface-2 absolute top-2 right-3 z-10 flex items-center gap-1 rounded-xl p-1 shadow-[var(--shadow)]">
          <Input
            autoFocus
            aria-label={t('terminal.search')}
            placeholder={t('terminal.search')}
            className="h-8 w-48"
            value={searchTerm}
            onChange={(e) => setSearchTerm(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') runSearch(e.shiftKey ? 'prev' : 'next');
              if (e.key === 'Escape') {
                setSearchOpen(false);
                termRef.current?.focus();
              }
            }}
          />
          <Button
            size="icon-sm"
            variant="ghost"
            aria-label={t('terminal.previous')}
            onClick={() => runSearch('prev')}
          >
            <ChevronUp className="size-4" />
          </Button>
          <Button
            size="icon-sm"
            variant="ghost"
            aria-label={t('terminal.next')}
            onClick={() => runSearch('next')}
          >
            <ChevronDown className="size-4" />
          </Button>
          <Button
            size="icon-sm"
            variant="ghost"
            aria-label={t('common.close')}
            onClick={() => {
              searchRef.current?.clearDecorations();
              setSearchOpen(false);
            }}
          >
            <X className="size-4" />
          </Button>
        </div>
      ) : null}

      <div
        ref={containerRef}
        className="terminal-host min-h-0 flex-1"
        onContextMenu={(e) => {
          if (!settingsRef.current.rightClickPaste) return;
          e.preventDefault();
          void navigator.clipboard
            .readText()
            .then((text) => {
              if (text) termRef.current?.paste(text);
            })
            .catch(() => undefined);
        }}
      />

      {tab.status === 'connecting' && !prompt ? (
        <div className="pointer-events-none absolute inset-x-0 top-0 flex justify-center pt-6">
          <div className="bg-surface-2 text-muted flex items-center gap-2 rounded-full px-4 py-2 text-[13px] shadow-[var(--shadow)]">
            <Loader2 className="size-4 animate-spin" />
            {t('terminal.connecting', { target: tab.title })}
          </div>
        </div>
      ) : null}

      {(tab.status === 'disconnected' || tab.status === 'error') && (
        <div
          className={cn(
            'border-border bg-surface-1 flex items-center justify-between gap-3 border-t px-4 py-2',
            tab.status === 'error' && 'bg-danger-soft',
          )}
        >
          <span className="text-muted min-w-0 truncate text-[13px]">
            {tab.status === 'error'
              ? (closedReason ?? t('terminal.connectionFailed'))
              : t('terminal.disconnected')}
          </span>
          <Button size="sm" variant="primary" onClick={() => setAttempt((a) => a + 1)}>
            <RotateCw className="size-4" />
            {t('terminal.reconnect')}
          </Button>
        </div>
      )}

      {bp === 'phone' || (bp === 'tablet' && !isTauri()) ? (
        <AccessoryBar
          onKey={(seq) => {
            const id = sessionRef.current;
            if (id) void commands.sessionWrite(id, seq);
            termRef.current?.focus();
          }}
          onFontSize={(delta) => {
            const term = termRef.current;
            if (!term) return;
            term.options.fontSize = Math.min(
              32,
              Math.max(8, (term.options.fontSize ?? 14) + delta),
            );
            fitRef.current?.fit();
          }}
        />
      ) : null}

      {prompt?.kind === 'hostKey' ? (
        <HostKeyDialog
          event={prompt.event}
          onAccept={(remember) => respond(true, { remember })}
          onReject={() => respond(false)}
        />
      ) : null}
      {prompt?.kind === 'auth' ? (
        <AuthPromptDialog
          event={prompt.event}
          onSubmit={(answers) => respond(true, { answers })}
          onCancel={() => respond(false)}
        />
      ) : null}
    </div>
  );
}

async function confirmAndOpen(uri: string): Promise<void> {
  let url: URL;
  try {
    url = new URL(uri);
  } catch {
    return;
  }
  if (!['http:', 'https:', 'mailto:'].includes(url.protocol)) return;
  const { ask } = await import('@tauri-apps/plugin-dialog');
  const ok = isTauri()
    ? await ask(`Open this link in your browser?\n\n${url.href}`, {
        title: 'Open link',
        kind: 'warning',
      })
    : window.confirm(`Open ${url.href}?`);
  if (ok) await openUrl(url.href);
}
