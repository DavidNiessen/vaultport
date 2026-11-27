// SPDX-License-Identifier: GPL-3.0-or-later
/**
 * Autocomplete for one terminal: follows shell integration marks, asks the core for
 * suggestions after the shell echoes what was typed, and accepts one by typing keys.
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

export interface PopupView {
  items: CompletionItem[];
  selected: number;
  /** Position relative to the terminal root element, in px. */
  left: number;
  top: number;
  /** Shown above the cursor (`top` is then the cursor row's top edge). */
  above: boolean;
}

export interface AutocompleteDeps {
  enabled: () => boolean;
  request: (input: InputLine, cwd: string | null) => Promise<CompletionItem[]>;
  send: (data: string) => void;
  onView: (view: PopupView | null) => void;
  root: () => HTMLElement | null;
}

export interface Autocomplete {
  /** Feeds what the user typed (from `onData`). */
  noteInput: (data: string) => void;
  /** Handles a key while the popup is open; returns `true` when it used the key. */
  handleKey: (e: KeyboardEvent) => boolean;
  pick: (index: number) => void;
  hover: (index: number) => void;
  close: () => void;
  dispose: () => void;
}

type Phase = 'unknown' | 'prompt' | 'input' | 'running' | 'idle';

/** Kinds after which a space separates the next word. */
const SPACED = new Set(['command', 'subcommand', 'option', 'value', 'path']);

/** The keys that accepting `item` types. */
export function acceptKeys(item: CompletionItem): string {
  const open = /[/=]$/.test(item.text);
  const space = SPACED.has(item.kind) && !open ? ' ' : '';
  return '\x7f'.repeat(item.deleteBefore) + '\x1b[3~'.repeat(item.deleteAfter) + item.text + space;
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
  const disposables: IDisposable[] = [];

  const setView = (v: PopupView | null) => {
    view = v;
    deps.onView(v);
  };
  const close = () => {
    seq++;
    if (timer) clearTimeout(timer);
    timer = null;
    if (view) setView(null);
  };

  const shellMark = (data: string) => {
    const [code, ...rest] = data.split(';');
    switch (code) {
      case 'A':
        phase = 'prompt';
        marker?.dispose();
        marker = null;
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

  const position = (): Pick<PopupView, 'left' | 'top' | 'above'> => {
    const screen = term.element?.querySelector<HTMLElement>('.xterm-screen');
    const root = deps.root();
    if (!screen || !root) return { left: 0, top: 0, above: false };
    const sr = screen.getBoundingClientRect();
    const rr = root.getBoundingClientRect();
    const cw = sr.width / term.cols;
    const ch = sr.height / term.rows;
    const { cursorX, cursorY } = term.buffer.active;
    const below = (cursorY + 1) * ch;
    const above = below + MAX_ITEMS * ROW_PX > sr.height && cursorY * ch > sr.height / 2;
    const left = Math.max(0, Math.min(sr.left - rr.left + cursorX * cw, rr.width - 320));
    const top = sr.top - rr.top + (above ? cursorY * ch : below);
    return { left, top, above };
  };

  const run = async () => {
    timer = null;
    if (!deps.enabled() || phase === 'running' || phase === 'prompt') {
      close();
      return;
    }
    const mark =
      phase === 'input' && marker?.isDisposed === false && marker.line >= 0
        ? { line: marker.line, col: markCol }
        : null;
    const input = readInput(term.buffer.active, mark);
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
    items = items.filter((i) => i.text !== word).slice(0, MAX_ITEMS);
    if (items.length === 0) {
      close();
      return;
    }
    setView({ items, selected: 0, ...position() });
  };

  const schedule = () => {
    if (timer) clearTimeout(timer);
    timer = setTimeout(() => void run(), DEBOUNCE_MS);
  };

  disposables.push(
    term.onWriteParsed(() => {
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

  return {
    noteInput: (data) => {
      if (!deps.enabled()) return;
      if (isTyping(data)) pending = true;
      else {
        pending = false;
        close();
      }
    },
    handleKey: (e) => {
      if (!view || e.type !== 'keydown' || e.altKey || e.ctrlKey || e.metaKey) return false;
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
          close();
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
    close,
    dispose: () => {
      close();
      marker?.dispose();
      term.textarea?.removeEventListener('blur', blur);
      for (const d of disposables) d.dispose();
    },
  };
}
