// SPDX-License-Identifier: GPL-3.0-or-later
/**
 * A terminal pane: an xterm.js instance bound to a core session. Output arrives on a Tauri
 * channel (base64 chunks, batched by the core); keystrokes go back through `session_write`.
 */
import { Channel } from '@tauri-apps/api/core';
import { readText, writeText } from '@tauri-apps/plugin-clipboard-manager';
import { openUrl } from '@tauri-apps/plugin-opener';
import { FitAddon } from '@xterm/addon-fit';
import { SearchAddon } from '@xterm/addon-search';
import { Unicode11Addon } from '@xterm/addon-unicode11';
import { WebLinksAddon } from '@xterm/addon-web-links';
import { WebglAddon } from '@xterm/addon-webgl';
import { Terminal } from '@xterm/xterm';
import { useQuery } from '@tanstack/react-query';
import {
  ArrowUpToLine,
  ChevronDown,
  Lightbulb,
  ChevronUp,
  Loader2,
  RotateCw,
  X,
} from 'lucide-react';
import { useCallback, useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';

import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { cn } from '@/lib/cn';
import { EMPTY } from '@/lib/empty';
import { commands, errorMessage, isTauri, type PathSource, type SessionEvent } from '@/lib/ipc';
import { useBreakpoint, useColorScheme } from '@/lib/platform';
import { useHosts, useSettings } from '@/lib/queries';
import { useApp, type TerminalTab } from '@/stores/app';

import { AccessoryBar } from './AccessoryBar';
import { AiCommandBar, type AiRequest } from './AiCommandBar';
import { AutocompletePopup, GhostText } from './autocomplete/AutocompletePopup';
import {
  attachAutocomplete,
  type Autocomplete,
  type GhostView,
  type PopupView,
} from './autocomplete/controller';
import { BlockMenu, type BlockMenuView } from './BlockMenu';
import { attachBlocks, blockText, formatDuration, type Block, type BlockTracker } from './blocks';
import { Composer } from './Composer';
import { ContextBar, type TermContextView } from './ContextBar';
import { closestCommand, fixCommandLine, isNotFound } from './didYouMean';
import { HistorySearch } from './HistorySearch';
import { attachInputHighlight, solidColor, type InputHighlighter } from './inputHighlight';
import { DEFAULT_AI_SETTINGS, type AiSettings } from '../ai/settings';
import { AuthPromptDialog, HostKeyDialog, type PendingPrompt } from './Prompts';
import { compileRules, highlightKeywords, type Rule } from './highlight';
import { DEFAULT_TERMINAL_SETTINGS, type TerminalSettings } from './settings';
import { minimumContrast, terminalBackdrop, themeColors } from './themes';
import { terminalRegistry, type TerminalAction } from './registry';
import { macEditingKeys } from './keys';
import { actionFor, useKeymap } from '../palette/shortcuts';

/** Where path suggestions can be listed for this tab. */
function pathSource(tab: TerminalTab): PathSource {
  if (tab.protocol === 'local') return 'local';
  // Remote files are read over an SFTP channel of the session's own connection.
  if (tab.protocol === 'ssh' || tab.protocol === 'mosh') return 'remote';
  return 'none';
}

/** Keys that clear the line being edited (end of line, then kill to its start). */
const CLEAR_LINE = '\x05\x15';

type Overlay =
  | { kind: 'history'; query: string; session: string[] }
  | { kind: 'ai'; request: AiRequest; selection: string }
  | { kind: 'composer'; initial: string };

let audio: AudioContext | null = null;

/** A short, soft beep for the audible bell. */
function playBell(): void {
  try {
    audio ??= new AudioContext();
    const osc = audio.createOscillator();
    const gain = audio.createGain();
    osc.frequency.value = 880;
    gain.gain.setValueAtTime(0.06, audio.currentTime);
    gain.gain.exponentialRampToValueAtTime(0.0001, audio.currentTime + 0.15);
    osc.connect(gain).connect(audio.destination);
    osc.start();
    osc.stop(audio.currentTime + 0.16);
  } catch {
    // No audio output.
  }
}

function decodeBase64(b64: string): Uint8Array {
  const bin = atob(b64);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}

export function TerminalView({ tab, active }: { tab: TerminalTab; active: boolean }) {
  const { t } = useTranslation();
  const rootRef = useRef<HTMLDivElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const acRef = useRef<Autocomplete | null>(null);
  const [acView, setAcView] = useState<PopupView | null>(null);
  const termRef = useRef<Terminal | null>(null);
  const fitRef = useRef<FitAddon | null>(null);
  const searchRef = useRef<SearchAddon | null>(null);
  const sessionRef = useRef<string | null>(null);
  const [prompt, setPrompt] = useState<PendingPrompt | null>(null);
  const [searchOpen, setSearchOpen] = useState(false);
  const [searchTerm, setSearchTerm] = useState('');
  const [searchOpts, setSearchOpts] = useState({
    caseSensitive: false,
    regex: false,
    wholeWord: false,
  });
  const [searchResults, setSearchResults] = useState<{ index: number; count: number } | null>(null);
  const [attempt, setAttempt] = useState(0);
  const [closedReason, setClosedReason] = useState<string | null>(null);
  const [ghost, setGhost] = useState<GhostView | null>(null);
  const [overlay, setOverlay] = useState<Overlay | null>(null);
  const [blockMenu, setBlockMenu] = useState<BlockMenuView | null>(null);
  const [sticky, setSticky] = useState<{ id: number; command: string } | null>(null);
  const [context, setContext] = useState<TermContextView | null>(null);
  /** Refreshes the context bar (directory changed, a command finished). */
  const contextRef = useRef<() => void>(() => undefined);
  /** A corrected command line after "command not found". */
  const [didYouMean, setDidYouMean] = useState<string | null>(null);
  /** When the visual bell last rang (restarts the flash animation). */
  const [bellFlash, setBellFlash] = useState(0);
  const blocksRef = useRef<BlockTracker | null>(null);
  const highlightRef = useRef<InputHighlighter | null>(null);
  /** Commands that exist on the machine (null: not known yet). */
  const knownRef = useRef<Set<string> | null>(null);
  /** Resolves when the shell answers the integration bootstrap. */
  const readyRef = useRef<((shell: string) => void) | null>(null);
  const { settings: ai } = useSettings<AiSettings>('ai', DEFAULT_AI_SETTINGS);
  const aiRef = useRef(ai);
  useEffect(() => {
    aiRef.current = ai;
  }, [ai]);
  const agents = useQuery({
    queryKey: ['agentStatus'],
    queryFn: () => commands.agentStatus(),
    staleTime: 30_000,
  });
  const agentsEnabled = agents.data?.settings.enabled ?? false;
  const updateTab = useApp((s) => s.updateTab);
  const theme = useApp((s) => s.theme);
  const scheme = useColorScheme(theme);
  const bp = useBreakpoint();
  const hosts = useHosts().data ?? EMPTY;
  const host = hosts.find((h) => h.id === tab.hostId);
  const { settings } = useSettings<TerminalSettings>('terminal', DEFAULT_TERMINAL_SETTINGS);
  const settingsRef = useRef(settings);
  /** The user's output highlighting rules, compiled. */
  const rulesRef = useRef<Rule[]>([]);
  useEffect(() => {
    settingsRef.current = settings;
    rulesRef.current = compileRules(settings.highlightRules);
  }, [settings]);

  /** Runs a terminal action; returns `false` when it does not apply right now. */
  const runActionRef = useRef<(action: TerminalAction, e?: KeyboardEvent) => boolean>(() => false);
  /** Called when a command block finishes (kept current by an effect). */
  const blockFinishedRef = useRef<(b: Block) => void>(() => undefined);

  const themeId = host?.config.terminal.theme ?? settings.theme;
  const preset = useApp((s) => s.preset);
  // Search matches use the preset colour for the current theme.
  const match = themeColors(themeId, scheme, preset).cursor ?? '#888888';
  const [zoom, setZoom] = useState(0);
  const baseFontSize = host?.config.terminal.fontSize ?? settings.fontSize;
  const fontSize = Math.min(40, Math.max(6, baseFontSize + zoom));

  // Create the terminal once.
  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    const term = new Terminal({
      allowProposedApi: true,
      // The default Pulse theme has a transparent background.
      allowTransparency: true,
      minimumContrastRatio: minimumContrast(themeId, scheme),
      fontFamily: `'${settingsRef.current.fontFamily}', 'JetBrains Mono', monospace`,
      fontSize,
      lineHeight: 1.15,
      cursorBlink: settingsRef.current.cursorBlink,
      cursorStyle: settingsRef.current.cursorStyle,
      scrollback: settingsRef.current.scrollback,
      drawBoldTextInBrightColors: settingsRef.current.brightBold,
      macOptionIsMeta: settingsRef.current.optionAsMeta,
      rightClickSelectsWord: false,
      overviewRuler: { width: 8 },
      theme: themeColors(themeId, scheme, preset),
      linkHandler: {
        activate: (_e, uri) => {
          void confirmAndOpen(uri);
        },
        allowNonHttpProtocols: false,
      },
    });
    const fit = new FitAddon();
    const search = new SearchAddon();
    search.onDidChangeResults(({ resultIndex, resultCount }) =>
      setSearchResults({ index: resultIndex, count: resultCount }),
    );
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

    // A pane of a workspace becomes the focused pane when its terminal gets focus.
    term.textarea?.addEventListener('focus', () => {
      const { tabs } = useApp.getState();
      const self = tabs.find((x) => x.id === tab.id);
      const ws =
        self?.kind === 'terminal' && self.workspace
          ? tabs.find((x) => x.id === self.workspace)
          : null;
      if (ws?.kind === 'workspace' && ws.focused !== tab.id) updateTab(ws.id, { focused: tab.id });
    });
    term.onSelectionChange(() => {
      if (settingsRef.current.copyOnSelect && term.hasSelection()) {
        void writeText(term.getSelection());
      }
    });
    const broadcastPanes = () => {
      const ws = useApp
        .getState()
        .tabs.find((x) => x.kind === 'workspace' && x.panes.includes(tab.id));
      return ws?.kind === 'workspace' && ws.broadcast ? ws.panes : null;
    };
    // Sends input to the session, or to every pane of the workspace when broadcasting.
    const send = (data: string) => {
      const panes = broadcastPanes();
      if (panes) {
        for (const pane of panes) {
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
    };
    const ac = attachAutocomplete(term, {
      // Other panes have different lines, so completing while broadcasting would garble them.
      enabled: () => settingsRef.current.autocomplete && !broadcastPanes(),
      mode: () => settingsRef.current.suggestMode,
      ghost: () => settingsRef.current.ghostText,
      request: async (input, cwd) =>
        (
          await commands.termComplete({
            hostId: tab.hostId ?? null,
            sessionId: sessionRef.current,
            line: input.line,
            cursor: input.cursor,
            cwd,
            paths: pathSource(tab),
          })
        ).items,
      send,
      onView: setAcView,
      onGhost: setGhost,
      root: () => rootRef.current,
    });
    acRef.current = ac;

    const colors = () => {
      const th = term.options.theme ?? {};
      return {
        ok: solidColor(th.green) ?? '#5fd69b',
        fail: solidColor(th.red) ?? '#f2687a',
        separator: solidColor(th.foreground) ?? '#888888',
      };
    };
    const blocks = attachBlocks(term, {
      enabled: () => settingsRef.current.commandBlocks,
      colors,
      onStarted: () => updateTab(tab.id, { activity: 'running' }),
      onFinished: (b) => blockFinishedRef.current(b),
      onBadge: (b, rect) => {
        const root = rootRef.current?.getBoundingClientRect();
        if (!root) return;
        setBlockMenu({ block: b, left: rect.right - root.left, top: rect.bottom - root.top + 4 });
      },
    });
    blocksRef.current = blocks;
    const highlighter = attachInputHighlight(term, {
      enabled: () => settingsRef.current.syntaxHighlight,
      ac,
      known: (name) => knownRef.current?.has(name) ?? null,
    });
    highlightRef.current = highlighter;
    // The answer to the shell integration bootstrap, and freezing colours of a line that runs.
    term.parser.registerOscHandler(633, (data) => {
      const ready = 'P;VaultportReady=';
      if (data.startsWith(ready)) {
        readyRef.current?.(data.slice(ready.length));
        readyRef.current = null;
        return true;
      }
      return false;
    });
    term.parser.registerOscHandler(133, (data) => {
      // Keep the colours of a line that runs, or that was abandoned for a new prompt.
      if (data.startsWith('C') || data.startsWith('A')) highlighter.freeze();
      return false;
    });
    // Directory changes (OSC 7 / 633 P) refresh the context bar once the handlers above ran.
    let contextTimer: ReturnType<typeof setTimeout> | null = null;
    const scheduleContext = () => {
      if (contextTimer) clearTimeout(contextTimer);
      contextTimer = setTimeout(() => contextRef.current(), 120);
    };
    term.parser.registerOscHandler(7, () => {
      scheduleContext();
      return false;
    });
    term.parser.registerOscHandler(633, (data) => {
      if (data.startsWith('P;Cwd=')) scheduleContext();
      return false;
    });
    const updateSticky = () => {
      const b = settingsRef.current.commandBlocks ? blocks.stickyAt() : null;
      setSticky((cur) =>
        b ? (cur?.id === b.id ? cur : { id: b.id, command: b.command }) : cur ? null : cur,
      );
    };
    term.onScroll(updateSticky);
    term.onWriteParsed(() => {
      if (term.buffer.active.viewportY !== term.buffer.active.baseY) updateSticky();
    });

    term.onData((data) => {
      setDidYouMean(null);
      ac.noteInput(data);
      // Enter: the line as coloured now is the command that runs.
      if (data === '\r') highlighter.freeze();
      else highlighter.noteTyping();
      send(data);
    });
    term.onBinary((data) => {
      const id = sessionRef.current;
      if (id) void commands.sessionWrite(id, data).catch(() => undefined);
    });
    term.onResize(({ cols, rows }) => {
      const id = sessionRef.current;
      if (id) void commands.sessionResize(id, cols, rows).catch(() => undefined);
    });
    let lastBell = 0;
    term.onBell(() => {
      const now = Date.now();
      if (now - lastBell < 300) return;
      lastBell = now;
      const mode = settingsRef.current.bell;
      if (mode === 'none') return;
      if (mode === 'sound') playBell();
      else setBellFlash(now);
      const current = useApp.getState().tabs.find((x) => x.id === tab.id);
      const { activeTab, tabs } = useApp.getState();
      const ws = tabs.find((x) => x.id === activeTab);
      const visible =
        activeTab === tab.id || (ws?.kind === 'workspace' && ws.panes.includes(tab.id));
      if (current?.kind === 'terminal' && current.activity !== 'running' && !visible) {
        updateTab(tab.id, { activity: 'bell' });
      }
    });
    term.onTitleChange((title) => {
      const current = useApp.getState().tabs.find((x) => x.id === tab.id);
      if (current?.kind === 'terminal' && current.customTitle) return;
      if (title && settingsRef.current.remoteTitles) updateTab(tab.id, { title });
    });
    term.attachCustomKeyEventHandler((e) => {
      if (ac.handleKey(e)) return false;
      if (e.type !== 'keydown') return true;
      // `# what you want` + Enter asks the local AI instead of running a comment.
      if (
        e.key === 'Enter' &&
        !e.shiftKey &&
        !e.ctrlKey &&
        !e.metaKey &&
        !e.altKey &&
        aiRef.current.enabled &&
        settingsRef.current.aiHashPrompt
      ) {
        // Only at a prompt the shell marked (shell integration): a heredoc or a Python REPL
        // line can start with `#` too.
        const line = ac.phase() === 'input' ? (ac.input()?.line.trim() ?? '') : '';
        if (/^#\s*\S/.test(line)) {
          e.preventDefault();
          send(CLEAR_LINE);
          setOverlay({
            kind: 'ai',
            request: { mode: 'ask', prompt: line.replace(/^#\s*/, '') },
            selection: '',
          });
          return false;
        }
      }
      const { keymap, apple, recording } = useKeymap.getState();
      if (recording) return true;
      const action = actionFor(e, keymap, apple, 'terminal');
      if (action && runActionRef.current(action as TerminalAction, e)) {
        e.preventDefault();
        return false;
      }
      const mod = e.ctrlKey || e.metaKey;
      if (mod && e.shiftKey && e.key.toLowerCase() === 'c' && term.hasSelection()) {
        void writeText(term.getSelection());
        return false;
      }
      if (e.metaKey && e.key.toLowerCase() === 'c' && term.hasSelection()) {
        void writeText(term.getSelection());
        return false;
      }
      const seq = apple ? macEditingKeys(e) : null;
      if (seq) {
        e.preventDefault();
        ac.noteInput(seq);
        send(seq);
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
      cwd: () => ac.cwd(),
      action: (a) => {
        runActionRef.current(a);
      },
    });
    const ro = new ResizeObserver(() => {
      if (el.offsetParent !== null) fit.fit();
    });
    ro.observe(el);
    requestAnimationFrame(() => fit.fit());
    return () => {
      ro.disconnect();
      highlighter.dispose();
      blocks.dispose();
      blocksRef.current = null;
      highlightRef.current = null;
      ac.dispose();
      acRef.current = null;
      terminalRegistry.delete(tab.id);
      term.dispose();
      termRef.current = null;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps -- created once per tab
  }, [tab.id]);

  /** Writes to this terminal's session only (never broadcast). */
  const writeSession = useCallback((data: string) => {
    const id = sessionRef.current;
    if (id) void commands.sessionWrite(id, data).catch(() => undefined);
  }, []);

  /** Replaces the line at the prompt with `text`, and runs it when `run` is set. */
  const putLine = useCallback(
    (text: string, run: boolean) => {
      const term = termRef.current;
      setOverlay(null);
      setDidYouMean(null);
      term?.focus();
      writeSession(CLEAR_LINE);
      if (text.includes('\n')) {
        // Bracketed paste keeps a multi-line command together.
        term?.paste(text);
        if (run) setTimeout(() => writeSession('\r'), 30);
      } else {
        writeSession(run ? `${text}\r` : text);
      }
    },
    [writeSession],
  );

  /** Loads shell integration into the running bash or zsh (see `term_integration_bootstrap`). */
  const installIntegration = useCallback(
    async (quiet: boolean) => {
      if (blocksRef.current?.active()) {
        if (!quiet) toast.success(t('terminal.integration.already'));
        return;
      }
      // The loader is typed at the prompt; anywhere else it would go to a running program.
      const term = termRef.current;
      if (term?.buffer.active.type !== 'normal' || !acRef.current?.input()) {
        if (!quiet) toast.error(t('terminal.integration.notAtPrompt'));
        return;
      }
      try {
        const b = await commands.termIntegrationBootstrap();
        const shell = await new Promise<string | null>((resolve) => {
          readyRef.current = resolve;
          setTimeout(() => {
            if (readyRef.current === resolve) {
              readyRef.current = null;
              resolve(null);
            }
          }, 6000);
          writeSession(b.start);
        });
        if (shell === null) {
          // If the shell reads the loader late, it would wait (with echo off) for a lone `.`:
          // send it now so typing never disappears.
          writeSession('.\n');
          if (!quiet) toast.error(t('terminal.integration.noAnswer'));
          return;
        }
        const lines = shell === 'bash' ? b.bash : shell === 'zsh' ? b.zsh : [];
        writeSession(`${lines.map((l) => `${l}\n`).join('')}.\n`);
        if (lines.length === 0) {
          if (!quiet) toast.error(t('terminal.integration.unsupported'));
        } else if (!quiet) toast.success(t('terminal.integration.enabled', { shell }));
      } catch (e) {
        if (!quiet) toast.error(errorMessage(e));
      }
    },
    [t, writeSession],
  );

  /** Fetches the commands on the machine, for highlighting unknown ones. */
  const loadKnownCommands = useCallback(async () => {
    for (let attempt = 0; attempt < 8; attempt++) {
      const id = sessionRef.current;
      const res = await commands
        .termCommands({ hostId: tab.hostId ?? null, sessionId: id, paths: pathSource(tab) })
        .catch(() => null);
      if (res?.complete) {
        const set = new Set(res.names);
        // Commands the user ran before are known too (aliases and functions included).
        const history = await commands.historyList(tab.hostId ?? null, 500).catch(() => []);
        for (const h of history) {
          const first = /^\s*(?:\w+=\S*\s+)*(\S+)/.exec(h.command)?.[1];
          if (first) set.add(first);
        }
        for (const b of blocksRef.current?.blocks() ?? []) {
          const first = b.command.trim().split(/\s+/)[0];
          if (first && b.exit === 0) set.add(first);
        }
        knownRef.current = set;
        return;
      }
      if (pathSource(tab) === 'none') return;
      await new Promise((r) => setTimeout(r, 1500 + attempt * 1000));
    }
  }, [tab]);

  const refreshContext = async (last?: TermContextView['last']) => {
    const cwd = acRef.current?.cwd() ?? null;
    if (!cwd || !settingsRef.current.contextBar) {
      setContext(null);
      return;
    }
    const res = await commands
      .termContext({
        hostId: tab.hostId ?? null,
        sessionId: sessionRef.current,
        paths: pathSource(tab),
        cwd,
      })
      .catch(() => null);
    setContext((cur) => ({
      cwd,
      gitHead: res?.gitHead ?? null,
      last: last === undefined ? (cur?.last ?? null) : last,
    }));
  };

  /** Whether this terminal is on screen (its tab, or the workspace it is a pane of). */
  const isVisible = () => {
    const { activeTab, tabs } = useApp.getState();
    if (activeTab === tab.id) return true;
    const ws = tabs.find((x) => x.id === activeTab);
    return ws?.kind === 'workspace' && ws.panes.includes(tab.id);
  };

  const onBlockFinished = (b: Block) => {
    const away = !document.hasFocus() || !isVisible();
    const failedRun = b.exit !== null && b.exit !== 0;
    updateTab(tab.id, { activity: away ? (failedRun ? 'failed' : 'done') : undefined });
    void refreshContext({
      exit: b.exit,
      ms: b.startedAt !== null && b.finishedAt !== null ? b.finishedAt - b.startedAt : null,
    });
    const first = b.command.trim().split(/\s+/)[0];
    if (first && b.exit === 0) knownRef.current?.add(first);
    const known = knownRef.current;
    if (first && known && !known.has(first)) {
      const output = blocksRef.current?.outputOf(b) ?? '';
      const fixed = isNotFound(b.exit, output) ? closestCommand(first, known) : null;
      setDidYouMean(fixed ? fixCommandLine(b.command.trim(), fixed) : null);
    } else setDidYouMean(null);
    const secs = settingsRef.current.notifyAfterSeconds;
    const ms = b.startedAt !== null && b.finishedAt !== null ? b.finishedAt - b.startedAt : 0;

    if (secs > 0 && ms >= secs * 1000 && away) {
      const failed = b.exit !== null && b.exit !== 0;
      const title = failed
        ? t('terminal.block.failedIn', { title: tab.title, code: b.exit ?? 1 })
        : t('terminal.block.finishedIn', { title: tab.title });
      const description = `${b.command} · ${formatDuration(ms)}`;
      const show = {
        label: t('terminal.block.show'),
        onClick: () => useApp.getState().activate(tab.id),
      };
      if (failed) toast.error(title, { description, action: show });
      else toast.success(title, { description, action: show });
    }
  };

  const currentLine = () => acRef.current?.input()?.line ?? '';
  const selection = () =>
    aiRef.current.useSelection ? (termRef.current?.getSelection() ?? '') : '';

  const runAction = (action: TerminalAction): boolean => {
    const term = termRef.current;
    if (!term) return false;
    const atPrompt = term.buffer.active.type === 'normal' && acRef.current?.phase() !== 'running';
    switch (action) {
      case 'search':
        setSearchOpen(true);
        return true;
      case 'clear':
        term.clear();
        acRef.current?.close();
        return true;
      case 'zoomIn':
        setZoom((z) => z + 1);
        return true;
      case 'zoomOut':
        setZoom((z) => z - 1);
        return true;
      case 'zoomReset':
        setZoom(0);
        return true;
      case 'paste':
        void readText()
          .then((text) => {
            if (text) term.paste(text);
          })
          .catch(() => undefined);
        return true;
      case 'suggest':
        if (!atPrompt) return false;
        acRef.current?.open();
        return true;
      case 'historySearch':
        // Full-screen programs and running commands keep their own Ctrl+R.
        if (!atPrompt) return false;
        acRef.current?.close();
        setOverlay({
          kind: 'history',
          query: currentLine().trim(),
          session: (blocksRef.current?.blocks() ?? []).map((b) => b.command).filter(Boolean),
        });
        return true;
      case 'aiCommand':
        if (!atPrompt) return false;
        acRef.current?.close();
        setOverlay({ kind: 'ai', request: { mode: 'ask', prompt: '' }, selection: selection() });
        return true;
      case 'composer':
        if (!atPrompt) return false;
        acRef.current?.close();
        setOverlay({ kind: 'composer', initial: currentLine() });
        return true;
      case 'prevBlock':
      case 'nextBlock': {
        const tracker = blocksRef.current;
        const b = tracker?.step(action === 'prevBlock' ? -1 : 1);
        if (!tracker || !b) {
          if (action === 'nextBlock' && tracker?.active()) term.scrollToBottom();
          return tracker?.active() ?? false;
        }
        tracker.reveal(b);
        return true;
      }
      case 'copyLastOutput': {
        const tracker = blocksRef.current;
        const b = tracker?.last();
        if (!tracker || !b) {
          toast(t('terminal.block.noBlocks'));
          return true;
        }
        void writeText(tracker.outputOf(b)).then(() => toast.success(t('terminal.block.copied')));
        return true;
      }
      case 'installIntegration':
        void installIntegration(false);
        return true;
      default:
        return false;
    }
  };

  useEffect(() => {
    runActionRef.current = runAction;
    blockFinishedRef.current = onBlockFinished;
    contextRef.current = () => void refreshContext();
  });

  // Live theme / font updates.
  useEffect(() => {
    const term = termRef.current;
    if (!term) return;
    term.options.theme = themeColors(themeId, scheme, preset);
    term.options.minimumContrastRatio = minimumContrast(themeId, scheme);
    term.options.fontSize = fontSize;
    term.options.fontFamily = `'${settings.fontFamily}', 'JetBrains Mono', monospace`;
    term.options.cursorBlink = settings.cursorBlink;
    term.options.cursorStyle = settings.cursorStyle;
    fitRef.current?.fit();
  }, [themeId, scheme, preset, fontSize, settings]);

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
          if (settingsRef.current.highlightKeywords) highlightKeywords(term, rulesRef.current);
          if (!document.hasFocus() || useApp.getState().activeTab !== tab.id) {
            updateTab(tab.id, { unread: true });
          }
          break;
        }
        case 'status':
          if (ev.status === 'connected') {
            updateTab(tab.id, { status: 'connected' });
            void loadKnownCommands();
            setTimeout(() => {
              if (disposed) return;
              if (
                (tab.protocol === 'ssh' || tab.protocol === 'mosh') &&
                settingsRef.current.sshIntegration === 'auto' &&
                !blocksRef.current?.active() &&
                term.buffer.active.type === 'normal'
              ) {
                void installIntegration(true);
              }
            }, 1500);
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
          ? ({ type: 'local', cwd: tab.cwd ?? null } as const)
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
    if (!active || !settings.autocomplete) acRef.current?.close();
  }, [active, settings.autocomplete]);

  useEffect(() => {
    if (active) {
      requestAnimationFrame(() => {
        fitRef.current?.fit();
        // In a split, only the focused pane takes the keyboard.
        const { tabs } = useApp.getState();
        const self = tabs.find((x) => x.id === tab.id);
        const ws =
          self?.kind === 'terminal' && self.workspace
            ? tabs.find((x) => x.id === self.workspace)
            : null;
        if (ws?.kind === 'workspace' && ws.focused && ws.focused !== tab.id) return;
        termRef.current?.focus();
      });
    }
  }, [active, tab.id]);

  const runSearch = useCallback(
    (dir: 'next' | 'prev' | 'live', term = searchTerm, options = searchOpts) => {
      const s = searchRef.current;
      if (!s) return;
      if (!term) {
        s.clearDecorations();
        setSearchResults(null);
        return;
      }
      const color = solidColor(match) ?? '#888888';
      const opts = {
        ...options,
        // Typing searches from the current match instead of jumping to the next one.
        incremental: dir === 'live',
        decorations: {
          matchOverviewRuler: '#888888',
          activeMatchColorOverviewRuler: color,
          matchBackground: `${color}55`,
          activeMatchBackground: color,
        },
      };
      try {
        if (dir === 'prev') s.findPrevious(term, opts);
        else s.findNext(term, opts);
      } catch {
        // An incomplete regular expression while typing.
        setSearchResults({ index: -1, count: 0 });
      }
    },
    [searchTerm, searchOpts, match],
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

  const background = terminalBackdrop(themeId, scheme, preset);
  const closeOverlay = () => {
    setOverlay(null);
    requestAnimationFrame(() => termRef.current?.focus());
  };
  return (
    <div ref={rootRef} className="relative flex h-full min-h-0 flex-col" style={{ background }}>
      {searchOpen ? (
        <div className="bg-surface-2 absolute top-2 right-3 z-10 flex items-center gap-1 rounded-xl p-1 shadow-[var(--shadow)]">
          <Input
            autoFocus
            aria-label={t('terminal.search')}
            placeholder={t('terminal.search')}
            className="h-8 w-48 font-mono"
            spellCheck={false}
            value={searchTerm}
            onChange={(e) => {
              setSearchTerm(e.target.value);
              runSearch('live', e.target.value);
            }}
            onKeyDown={(e) => {
              if (e.key === 'Enter') runSearch(e.shiftKey ? 'prev' : 'next');
              if (e.key === 'Escape') {
                searchRef.current?.clearDecorations();
                setSearchOpen(false);
                termRef.current?.focus();
              }
            }}
          />
          <span
            className="text-subtle min-w-12 text-center text-[11px] tabular-nums"
            aria-live="polite"
          >
            {searchTerm && searchResults
              ? searchResults.count === 0
                ? t('terminal.searchNone')
                : `${String(searchResults.index + 1)}/${String(searchResults.count)}`
              : ''}
          </span>
          {(
            [
              ['caseSensitive', 'Aa', t('terminal.searchCase')],
              ['wholeWord', 'ab', t('terminal.searchWord')],
              ['regex', '.*', t('terminal.searchRegex')],
            ] as const
          ).map(([key, label, title]) => (
            <button
              key={key}
              type="button"
              title={title}
              aria-label={title}
              aria-pressed={searchOpts[key]}
              onClick={() => {
                const next = { ...searchOpts, [key]: !searchOpts[key] };
                setSearchOpts(next);
                runSearch('live', searchTerm, next);
              }}
              className={cn(
                'flex h-7 min-w-7 items-center justify-center rounded-md px-1 font-mono text-[12px]',
                searchOpts[key]
                  ? 'bg-accent-soft text-accent-text'
                  : 'text-muted hover:bg-surface-3',
                key === 'wholeWord' && 'underline decoration-dotted underline-offset-2',
              )}
            >
              {label}
            </button>
          ))}
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
              termRef.current?.focus();
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

      {bellFlash ? (
        <div
          key={bellFlash}
          aria-hidden
          className="vp-bell-flash pointer-events-none absolute inset-0 z-10"
          onAnimationEnd={() => setBellFlash(0)}
        />
      ) : null}

      {sticky && !searchOpen ? (
        <button
          type="button"
          data-testid="sticky-command"
          onClick={() => {
            const b = blocksRef.current?.blocks().find((x) => x.id === sticky.id);
            if (b) blocksRef.current?.reveal(b);
          }}
          className="bg-surface-2/90 border-border text-muted hover:text-text absolute top-0 right-4 left-2 z-10 flex h-7 items-center gap-2 truncate rounded-b-lg border border-t-0 px-3 text-left font-mono text-[12px] shadow-[var(--shadow)] backdrop-blur-md"
          title={t('terminal.block.jumpToCommand')}
        >
          <ArrowUpToLine className="size-3.5 shrink-0" aria-hidden />
          <span className="truncate">{sticky.command}</span>
        </button>
      ) : null}

      {didYouMean && !overlay ? (
        <div
          role="status"
          data-testid="did-you-mean"
          className="bg-surface-2/95 border-border animate-fade-in absolute right-4 bottom-3 z-20 flex max-w-[calc(100%-2rem)] items-center gap-2 rounded-full border py-1 pr-1 pl-3 text-[13px] shadow-[var(--shadow)] backdrop-blur-md"
        >
          <Lightbulb className="text-warning size-4 shrink-0" aria-hidden />
          <span className="text-muted shrink-0">{t('terminal.didYouMean')}</span>
          <code className="text-text min-w-0 truncate font-mono">{didYouMean}</code>
          <Button size="sm" variant="primary" onClick={() => putLine(didYouMean, true)}>
            {t('terminal.run')}
          </Button>
          <Button size="sm" variant="ghost" onClick={() => putLine(didYouMean, false)}>
            {t('terminal.edit')}
          </Button>
          <Button
            size="icon-sm"
            variant="ghost"
            aria-label={t('common.close')}
            onClick={() => {
              setDidYouMean(null);
              termRef.current?.focus();
            }}
          >
            <X className="size-3.5" />
          </Button>
        </div>
      ) : null}

      {ghost && !overlay ? (
        <GhostText
          {...ghost}
          fontFamily={`'${settings.fontFamily}', 'JetBrains Mono', monospace`}
          fontSize={fontSize}
          color={themeColors(themeId, scheme, preset).foreground ?? '#888888'}
        />
      ) : null}

      {acView && !overlay ? (
        <AutocompletePopup
          view={acView}
          onPick={(i) => acRef.current?.pick(i)}
          onHover={(i) => acRef.current?.hover(i)}
        />
      ) : null}

      {overlay?.kind === 'history' ? (
        <HistorySearch
          hostId={tab.hostId}
          sessionCommands={overlay.session}
          initialQuery={overlay.query}
          onInsert={(c) => putLine(c, false)}
          onRun={(c) => putLine(c, true)}
          onClose={closeOverlay}
        />
      ) : null}
      {overlay?.kind === 'ai' ? (
        <AiCommandBar
          request={overlay.request}
          enabled={ai.enabled}
          sessionId={tab.sessionId}
          selection={overlay.selection}
          onInsert={(c) => putLine(c, false)}
          onRun={(c) => putLine(c, true)}
          onClose={closeOverlay}
          onOpenSettings={() => {
            closeOverlay();
            useApp.getState().setSection('settings');
          }}
        />
      ) : null}
      {overlay?.kind === 'composer' ? (
        <Composer
          initial={overlay.initial}
          fontFamily={`'${settings.fontFamily}', 'JetBrains Mono', monospace`}
          onInsert={(c) => putLine(c, false)}
          onRun={(c) => putLine(c, true)}
          onClose={closeOverlay}
        />
      ) : null}
      {blockMenu ? (
        <BlockMenu
          view={blockMenu}
          aiEnabled={ai.enabled}
          agentsEnabled={agentsEnabled}
          onClose={() => {
            setBlockMenu(null);
            termRef.current?.focus();
          }}
          onCopyCommand={() => void writeText(blockMenu.block.command)}
          onCopyOutput={() =>
            void writeText(blocksRef.current?.outputOf(blockMenu.block) ?? '').then(() =>
              toast.success(t('terminal.block.copied')),
            )
          }
          onCopyBoth={() =>
            void writeText(
              blockText(
                blockMenu.block.command,
                blocksRef.current?.outputOf(blockMenu.block) ?? '',
              ),
            ).then(() => toast.success(t('terminal.block.copied')))
          }
          onRerun={() => putLine(blockMenu.block.command, true)}
          onFix={() => setOverlay({ kind: 'ai', request: { mode: 'fix' }, selection: '' })}
          onExplain={() =>
            void writeText(
              t('terminal.block.agentPrompt', {
                title: tab.title,
                id: tab.sessionId ?? '',
                command: blockMenu.block.command,
              }),
            ).then(() => toast.success(t('terminal.block.agentPromptCopied')))
          }
        />
      ) : null}

      {tab.status === 'connecting' && !prompt ? (
        <div className="pointer-events-none absolute inset-x-0 top-0 flex justify-center pt-6">
          <div className="bg-surface-2 text-muted flex items-center gap-2 rounded-full px-4 py-2 text-[13px] shadow-[var(--shadow)]">
            <Loader2 className="size-4 animate-spin" />
            {t('terminal.connecting', { target: tab.title })}
          </div>
        </div>
      ) : null}

      {/* Space is kept from the start so the terminal does not resize when the bar fills in. */}
      {settings.contextBar &&
      bp !== 'phone' &&
      tab.protocol !== 'serial' &&
      tab.protocol !== 'telnet' ? (
        <ContextBar view={context} />
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
          onFontSize={(delta) => setZoom((z) => z + delta)}
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
