// SPDX-License-Identifier: GPL-3.0-or-later
/**
 * Autocomplete for one terminal: follows shell integration marks, asks the core for
 * suggestions after the shell echoes what was typed, and accepts one by typing keys.
 *
 * Two things are shown: the suggestion list at the cursor, and "ghost text" — the rest of the
 * best matching command from history, drawn dimmed after the cursor like fish and Warp do (→
 * or End accepts it). In `auto` mode the list opens while typing; in `tab` mode it opens only
 * on Tab (or Ctrl+Space), and Tab goes to the shell when there is nothing to suggest.
 */
import type { IDisposable, IMarker, Terminal } from '@xterm/xterm';

import type { CompletionItem } from '@/lib/ipc';

import { readInput, wordBefore, type InputLine } from './inputLine';

/** Rows shown at most. */
export const MAX_ITEMS = 8;
/** Wait after the echo before asking, so fast typing sends one request. */
const DEBOUNCE_MS = 60;
/** Popup row height in px, for deciding whether it fits below the cursor. */
const ROW_PX = 30;
/** Extra height of the details footer. */
const FOOTER_PX = 56;

export interface PopupView {
  items: CompletionItem[];
  selected: number;
  /** The word being completed, for highlighting matches. */
  word: string;
  /** Position relative to the terminal root element, in px. */
  left: number;
  top: number;
  /** Shown above the cursor (`top` is then the cursor row's top edge). */
  above: boolean;
}

export interface GhostView {
  text: string;
  /** Position of the cursor cell relative to the terminal root element, in px. */
  left: number;
  top: number;
  height: number;
}

export type SuggestMode = 'auto' | 'tab';

export interface AutocompleteDeps {
  enabled: () => boolean;
  mode: () => SuggestMode;
  ghost: () => boolean;
  request: (input: InputLine, cwd: string | null) => Promise<CompletionItem[]>;
  send: (data: string) => void;
  onView: (view: PopupView | null) => void;
  onGhost: (ghost: GhostView | null) => void;
  root: () => HTMLElement | null;
}

/** Where the shell is, from its integration marks. */
export type Phase = 'unknown' | 'prompt' | 'input' | 'running' | 'idle';

export interface Autocomplete {
  /** Feeds what the user typed (from `onData`). */
  noteInput: (data: string) => void;
  /** Handles a key; returns `true` when it used the key. */
  handleKey: (e: KeyboardEvent) => boolean;
  pick: (index: number) => void;
  hover: (index: number) => void;
  /** Opens the list for the current input (Ctrl+Space). */
  open: () => void;
  close: () => void;
  /** The shell's working directory, from OSC 7 / OSC 633 P. */
  cwd: () => string | null;
  /** Where the shell is (`unknown` without shell integration). */
  phase: () => Phase;
  /** The command line being typed, or `null` when none can be read. */
  input: () => InputLine | null;
  /** Where input started (OSC 133;B), if known. */
  inputMark: () => { line: number; col: number } | null;
  dispose: () => void;
}

/** Kinds after which a space separates the next word. */
const SPACED = new Set(['command', 'subcommand', 'option', 'value', 'path']);

/** The keys that accepting `item` types. */
export function acceptKeys(item: CompletionItem): string {
  const open = /[/=]$/.test(item.text);
  const space = SPACED.has(item.kind) && !open ? ' ' : '';
  return '\x7f'.repeat(item.deleteBefore) + '\x1b[3~'.repeat(item.deleteAfter) + item.text + space;
}

/**
 * The ghost text for `input`: the rest of the best history suggestion that continues the
 * whole line. Only offered with the cursor at the end of the line.
 */
export function ghostFor(input: InputLine, items: CompletionItem[]): string | null {
  if (input.cursor !== input.line.length || !input.line.trim()) return null;
  for (const item of items) {
    if (item.kind !== 'history' || item.deleteAfter !== 0) continue;
    const head = input.line.slice(0, input.line.length - item.deleteBefore);
    const full = head + item.text;
    if (full.length > input.line.length && full.startsWith(input.line)) {
      return full.slice(input.line.length);
    }
  }
  return null;
}

/** The first word of `ghost` (with the spaces before it), for accepting word by word. */
export function ghostWord(ghost: string): string {
  const m = /^\s*\S+/.exec(ghost);
  return m ? m[0] : ghost;
}

/** Whether `data` from `onData` is typing that may change the word being completed. */
function isTyping(data: string): boolean {
  if (data === '\x7f' || data === '\b') return true;
  if (data.length === 0 || data.length > 64) return false;
  for (let i = 0; i < data.length; i++) if (data.charCodeAt(i) < 0x20) return false;
  return true;
}

export function attachAutocomplete(term: Terminal, deps: AutocompleteDeps): Autocomplete {
  let phase: Phase = 'unknown';
  let marker: IMarker | null = null;
  let markCol = 0;
  let cwd: string | null = null;
  let pending = false;
  let timer: ReturnType<typeof setTimeout> | null = null;
  let seq = 0;
  let view: PopupView | null = null;
  let ghost: string | null = null;
  /** Suggestions for the input as it is now (in `tab` mode, shown on Tab). */
  let ready: { line: string; cursor: number; items: CompletionItem[]; word: string } | null = null;
  /** Open the list as soon as the next answer arrives (Tab / Ctrl+Space). */
  let wantOpen = false;
  /** The ghost text must move to the cursor after the next write. */
  let reposition = false;
  const disposables: IDisposable[] = [];

  const setView = (v: PopupView | null) => {
    view = v;
    deps.onView(v);
  };
  const setGhost = (g: string | null) => {
    if (g === ghost) return;
    ghost = g;
    if (!g) {
      deps.onGhost(null);
      return;
    }
    const p = cellPosition();
    deps.onGhost(p ? { text: g, ...p } : null);
  };
  const close = () => {
    seq++;
    if (timer) clearTimeout(timer);
    timer = null;
    wantOpen = false;
    ready = null;
    if (view) setView(null);
    setGhost(null);
  };

  const shellMark = (data: string) => {
    const [code, ...rest] = data.split(';');
    switch (code) {
      case 'A':
        phase = 'prompt';
        marker?.dispose();
        marker = null;
        close();
        break;
      case 'B':
        phase = 'input';
        marker?.dispose();
        marker = term.registerMarker(0);
        markCol = term.buffer.active.cursorX;
        break;
      case 'C':
        phase = 'running';
        close();
        break;
      case 'D':
        phase = 'idle';
        break;
      case 'P': {
        const prop = rest.join(';');
        if (prop.startsWith('Cwd=')) cwd = prop.slice(4) || null;
        break;
      }
    }
    return false;
  };
  disposables.push(term.parser.registerOscHandler(133, shellMark));
  disposables.push(term.parser.registerOscHandler(633, shellMark));
  disposables.push(
    term.parser.registerOscHandler(7, (data) => {
      try {
        const url = new URL(data);
        if (url.protocol === 'file:') cwd = decodeURIComponent(url.pathname) || null;
      } catch {
        // Not a URL: ignore.
      }
      return false;
    }),
  );

  const metrics = () => {
    const screen = term.element?.querySelector<HTMLElement>('.xterm-screen');
    const root = deps.root();
    if (!screen || !root) return null;
    const sr = screen.getBoundingClientRect();
    const rr = root.getBoundingClientRect();
    return { sr, rr, cw: sr.width / term.cols, ch: sr.height / term.rows };
  };

  const cellPosition = (): Omit<GhostView, 'text'> | null => {
    const m = metrics();
    if (!m) return null;
    const { cursorX, cursorY } = term.buffer.active;
    return {
      left: m.sr.left - m.rr.left + cursorX * m.cw,
      top: m.sr.top - m.rr.top + cursorY * m.ch,
      height: m.ch,
    };
  };

  const position = (): Pick<PopupView, 'left' | 'top' | 'above'> => {
    const m = metrics();
    if (!m) return { left: 0, top: 0, above: false };
    const { sr, rr, cw, ch } = m;
    const { cursorX, cursorY } = term.buffer.active;
    const below = (cursorY + 1) * ch;
    const height = MAX_ITEMS * ROW_PX + FOOTER_PX;
    const above = below + height > sr.height && cursorY * ch > sr.height / 2;
    const left = Math.max(0, Math.min(sr.left - rr.left + cursorX * cw, rr.width - 448));
    const top = sr.top - rr.top + (above ? cursorY * ch : below);
    return { left, top, above };
  };

  const currentInput = (): InputLine | null => {
    if (phase === 'running' || phase === 'prompt') return null;
    const mark =
      phase === 'input' && marker?.isDisposed === false && marker.line >= 0
        ? { line: marker.line, col: markCol }
        : null;
    return readInput(term.buffer.active, mark);
  };

  const show = (items: CompletionItem[], word: string) => {
    const visible = items.filter((i) => i.text !== word).slice(0, MAX_ITEMS);
    if (visible.length === 0) {
      if (view) setView(null);
      return false;
    }
    setView({ items: visible, selected: 0, word, ...position() });
    return true;
  };

  const run = async () => {
    timer = null;
    if (!deps.enabled() || phase === 'running' || phase === 'prompt') {
      close();
      return;
    }
    const input = currentInput();
    if (!input?.line.trim()) {
      close();
      return;
    }
    const mine = ++seq;
    let items: CompletionItem[];
    try {
      items = await deps.request(input, cwd);
    } catch {
      items = [];
    }
    if (mine !== seq) return;
    const word = wordBefore(input);
    ready = { line: input.line, cursor: input.cursor, items, word };
    setGhost(deps.ghost() ? ghostFor(input, items) : null);
    if (deps.mode() === 'auto' || wantOpen || view) {
      wantOpen = false;
      show(items, word);
    }
  };

  const schedule = () => {
    if (timer) clearTimeout(timer);
    timer = setTimeout(() => void run(), DEBOUNCE_MS);
  };

  disposables.push(
    term.onWriteParsed(() => {
      if (reposition) {
        reposition = false;
        const g = ghost;
        ghost = null;
        setGhost(g);
      }
      if (!pending) return;
      pending = false;
      schedule();
    }),
  );
  disposables.push(term.onResize(close));
  disposables.push(term.onScroll(close));
  const blur = () => close();
  term.textarea?.addEventListener('blur', blur);

  const accept = (index: number) => {
    const item = view?.items[index];
    if (!item) return;
    close();
    const keys = acceptKeys(item);
    pending = /[/=]$/.test(item.text);
    deps.send(keys);
  };

  const acceptGhost = (text: string) => {
    setGhost(null);
    pending = true;
    deps.send(text);
  };

  /** The suggestions computed for exactly the input on screen, if any. */
  const readyNow = () => {
    const input = currentInput();
    if (!ready || !input) return null;
    return ready.line === input.line && ready.cursor === input.cursor ? ready : null;
  };

  return {
    noteInput: (data) => {
      if (!deps.enabled()) return;
      if (isTyping(data)) {
        pending = true;
        // The ghost stays only while the typed text continues it.
        if (ghost && data.length === 1 && data !== '\x7f' && ghost.startsWith(data)) {
          // Moved to the new cursor cell once the shell has echoed the key.
          const rest = ghost.slice(data.length);
          if (rest) {
            ghost = rest;
            reposition = true;
          } else setGhost(null);
        } else {
          setGhost(null);
        }
      } else {
        pending = false;
        close();
      }
    },
    handleKey: (e) => {
      if (e.type !== 'keydown') return false;
      const plain = !e.altKey && !e.ctrlKey && !e.metaKey;
      if (ghost && !e.shiftKey) {
        if (plain && (e.key === 'ArrowRight' || e.key === 'End')) {
          if (view) setView(null);
          acceptGhost(ghost);
          e.preventDefault();
          return true;
        }
        // Word by word: Ctrl+→ (Option+→ on macOS, which otherwise jumps a word).
        if ((e.ctrlKey || e.altKey) && !e.metaKey && e.key === 'ArrowRight') {
          if (view) setView(null);
          acceptGhost(ghostWord(ghost));
          e.preventDefault();
          return true;
        }
      }
      if (!view) {
        if (plain && !e.shiftKey && e.key === 'Tab' && deps.mode() === 'tab' && deps.enabled()) {
          const r = readyNow();
          if (r && show(r.items, r.word)) {
            e.preventDefault();
            e.stopPropagation();
            return true;
          }
        }
        return false;
      }
      if (!plain) return false;
      const n = view.items.length;
      switch (e.key) {
        case 'ArrowDown':
          setView({ ...view, selected: (view.selected + 1) % n });
          break;
        case 'ArrowUp':
          setView({ ...view, selected: (view.selected - 1 + n) % n });
          break;
        case 'Tab':
          accept(view.selected);
          break;
        case 'Escape':
          // Hides the list; the inline suggestion stays.
          wantOpen = false;
          setView(null);
          break;
        case 'Enter':
          close();
          return false;
        default:
          return false;
      }
      e.preventDefault();
      e.stopPropagation();
      return true;
    },
    pick: accept,
    hover: (index) => {
      if (view && index !== view.selected) setView({ ...view, selected: index });
    },
    open: () => {
      if (!deps.enabled()) return;
      const r = readyNow();
      if (r && show(r.items, r.word)) return;
      wantOpen = true;
      schedule();
    },
    close,
    cwd: () => cwd,
    phase: () => phase,
    input: currentInput,
    inputMark: () =>
      phase === 'input' && marker?.isDisposed === false && marker.line >= 0
        ? { line: marker.line, col: markCol }
        : null,
    dispose: () => {
      close();
      marker?.dispose();
      term.textarea?.removeEventListener('blur', blur);
      for (const d of disposables) d.dispose();
    },
  };
}
