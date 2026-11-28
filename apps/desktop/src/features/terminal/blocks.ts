// SPDX-License-Identifier: GPL-3.0-or-later
/**
 * Command blocks in the terminal, Warp-style: with shell integration (OSC 133 / 633) every
 * command and its output form a block. Blocks get a thin separator above their prompt and a
 * badge with the exit status and duration at the right edge; failed commands are marked in
 * the overview ruler. The tracker lets the UI jump between blocks, copy a block's command or
 * output, show the command of the block being scrolled through and react when a long command
 * finishes. Nothing is drawn without shell integration.
 */
import type { IDecoration, IDisposable, IMarker, Terminal } from '@xterm/xterm';

/** Blocks kept (older ones are dropped with their decorations). */
const MAX_BLOCKS = 500;

export interface Block {
  id: number;
  /** Where the prompt starts (OSC 133;A). */
  prompt: IMarker;
  /** First line of output (OSC 133;C). */
  output: IMarker | null;
  /** Where the command finished (OSC 133;D). */
  end: IMarker | null;
  command: string;
  exit: number | null;
  startedAt: number | null;
  finishedAt: number | null;
  cwd: string | null;
  decorations: IDecoration[];
}

export interface BlockEvents {
  /** A command finished. */
  onFinished?: (block: Block) => void;
  /** The badge of a block was clicked. */
  onBadge?: (block: Block, anchor: DOMRect) => void;
  /** Colours for the decorations (`#rrggbb`). */
  colors: () => { ok: string; fail: string; separator: string };
  enabled: () => boolean;
}

export interface BlockTracker {
  blocks: () => readonly Block[];
  /** Whether the shell reported integration marks. */
  active: () => boolean;
  /** The block before / after the one at the top of the viewport (or the selected one). */
  step: (dir: -1 | 1) => Block | null;
  /** Scrolls to a block and highlights it briefly. */
  reveal: (block: Block) => void;
  /** The finished block with output closest to the end, if any. */
  last: () => Block | null;
  /** Plain text output of a block. */
  outputOf: (block: Block) => string;
  /** The block whose output fills the top of the viewport while its command is scrolled out. */
  stickyAt: () => Block | null;
  /** The block containing absolute buffer row `row`. */
  at: (row: number) => Block | null;
  dispose: () => void;
}

/** "1.2s", "3m 04s", "1h 02m". */
export function formatDuration(ms: number): string {
  if (ms < 1000) return `${Math.max(0, Math.round(ms))}ms`;
  const s = ms / 1000;
  if (s < 60) return `${s < 10 ? s.toFixed(1) : Math.round(s)}s`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m}m ${String(Math.round(s % 60)).padStart(2, '0')}s`;
  return `${Math.floor(m / 60)}h ${String(m % 60).padStart(2, '0')}m`;
}

/** Text of a block's badge. */
export function badgeText(exit: number | null, ms: number | null): string {
  const mark = exit === null ? '•' : exit === 0 ? '✓' : `✗ ${exit}`;
  return ms === null ? mark : `${mark} · ${formatDuration(ms)}`;
}

/** A command block as plain text for the clipboard. */
export function blockText(command: string, output: string, prompt = '$'): string {
  return output ? `${prompt} ${command}\n${output}` : `${prompt} ${command}`;
}

/** Decodes the `\xHH` and `\\` escapes of an OSC 633;E command line. */
export function unescapeCommand(s: string): string {
  return s.replace(/\\(x[0-9a-fA-F]{2}|\\)/g, (_, e: string) =>
    e === '\\' ? '\\' : String.fromCharCode(parseInt(e.slice(1), 16)),
  );
}

export function attachBlocks(term: Terminal, ev: BlockEvents): BlockTracker {
  const blocks: Block[] = [];
  let current: Block | null = null;
  let pendingCommand: string | null = null;
  /** Where input started (OSC 133;B), to read the command when the shell does not send it. */
  let inputMark: { marker: IMarker; col: number } | null = null;
  let cwd: string | null = null;
  let seen = false;
  let nextId = 1;
  let selected: Block | null = null;
  let flash: IDecoration | null = null;
  const disposables: IDisposable[] = [];

  const drop = (b: Block) => {
    for (const d of b.decorations) d.dispose();
    b.prompt.dispose();
    b.output?.dispose();
    b.end?.dispose();
  };

  const readTyped = (): string => {
    if (!inputMark || inputMark.marker.isDisposed) return '';
    const buf = term.buffer.active;
    const startRow = inputMark.marker.line;
    // At C the cursor is on the first output row; the command is on the rows before it.
    const cursorRow = buf.baseY + buf.cursorY;
    const last = cursorRow > startRow ? cursorRow - 1 : startRow;
    let text = '';
    for (let y = startRow; y <= last; y++) {
      const s = buf.getLine(y)?.translateToString(true) ?? '';
      text +=
        y === startRow ? s.slice(inputMark.col) : `${buf.getLine(y)?.isWrapped ? '' : '\n'}${s}`;
    }
    return text.trim();
  };

  const decorate = (b: Block) => {
    if (!ev.enabled()) return;
    const colors = ev.colors();
    const failed = b.exit !== null && b.exit !== 0;
    // Separator: a hairline along the top of the prompt row.
    const sep = term.registerDecoration({ marker: b.prompt, x: 0, width: term.cols });
    sep?.onRender((el) => {
      el.classList.add('vp-block-sep');
      el.style.setProperty('--vp-sep', colors.separator);
      if (failed) el.style.setProperty('--vp-sep', colors.fail);
    });
    if (sep) b.decorations.push(sep);
    const ms = b.startedAt !== null && b.finishedAt !== null ? b.finishedAt - b.startedAt : null;
    const text = badgeText(b.exit, ms);
    const width = Math.min(term.cols - 1, Array.from(text).length + 2);
    const badge = term.registerDecoration({
      marker: b.prompt,
      anchor: 'right',
      // xterm only applies a right offset that is not 0.
      x: 1,
      width,
      ...(failed
        ? { overviewRulerOptions: { color: colors.fail, position: 'full' as const } }
        : {}),
    });
    badge?.onRender((el) => {
      if (el.dataset.vp) return;
      el.dataset.vp = '1';
      el.classList.add('vp-block-badge', failed ? 'vp-fail' : 'vp-ok');
      el.style.setProperty('--vp-color', failed ? colors.fail : colors.ok);
      el.textContent = text;
      el.title = b.command;
      el.addEventListener('mousedown', (e) => {
        e.preventDefault();
        e.stopPropagation();
      });
      el.addEventListener('click', (e) => {
        e.stopPropagation();
        ev.onBadge?.(b, el.getBoundingClientRect());
      });
    });
    if (badge) b.decorations.push(badge);
  };

  const onMark = (data: string, vscode: boolean) => {
    const [code, ...rest] = data.split(';');
    switch (code) {
      case 'A': {
        seen = true;
        if (current?.output && current.end === null) {
          // Finished without D (e.g. Ctrl+C in some shells).
          current.end = term.registerMarker(0);
          current.finishedAt = Date.now();
          decorate(current);
        }
        const prompt = term.registerMarker(0);
        const b: Block = {
          id: nextId++,
          prompt,
          output: null,
          end: null,
          command: '',
          exit: null,
          startedAt: null,
          finishedAt: null,
          cwd,
          decorations: [],
        };
        prompt.onDispose(() => {
          const i = blocks.indexOf(b);
          if (i >= 0) blocks.splice(i, 1);
          for (const d of b.decorations) d.dispose();
        });
        blocks.push(b);
        while (blocks.length > MAX_BLOCKS) {
          const old = blocks.shift();
          if (old) drop(old);
        }
        current = b;
        pendingCommand = null;
        break;
      }
      case 'B': {
        inputMark?.marker.dispose();
        inputMark = { marker: term.registerMarker(0), col: term.buffer.active.cursorX };
        break;
      }
      case 'C': {
        if (!current) break;
        current.output = term.registerMarker(0);
        current.startedAt = Date.now();
        current.command = pendingCommand ?? readTyped();
        current.cwd = cwd;
        inputMark?.marker.dispose();
        inputMark = null;
        break;
      }
      case 'D': {
        if (!current || current.end) break;
        const exit = Number.parseInt(rest[0] ?? '', 10);
        current.exit = Number.isNaN(exit) ? null : exit;
        current.end = term.registerMarker(0);
        current.finishedAt = Date.now();
        // An empty prompt (Enter on an empty line) is not worth a badge.
        if (current.output && current.command) {
          decorate(current);
          ev.onFinished?.(current);
        }
        break;
      }
      case 'E':
        if (vscode) pendingCommand = unescapeCommand(rest.join(';'));
        break;
      case 'P': {
        const prop = rest.join(';');
        if (vscode && prop.startsWith('Cwd=')) cwd = prop.slice(4) || null;
        break;
      }
    }
    return false;
  };
  disposables.push(term.parser.registerOscHandler(133, (d) => onMark(d, false)));
  disposables.push(term.parser.registerOscHandler(633, (d) => onMark(d, true)));
  disposables.push(
    term.parser.registerOscHandler(7, (data) => {
      try {
        const url = new URL(data);
        if (url.protocol === 'file:') cwd = decodeURIComponent(url.pathname) || null;
      } catch {
        // ignore
      }
      return false;
    }),
  );

  const lineText = (y: number) => term.buffer.active.getLine(y)?.translateToString(true) ?? '';

  const outputOf = (b: Block): string => {
    if (!b.output || b.output.isDisposed) return '';
    const endRow = b.end && !b.end.isDisposed ? b.end.line : term.buffer.active.length;
    const lines: string[] = [];
    for (let y = b.output.line; y < endRow; y++) {
      const text = lineText(y);
      const prev = lines.length - 1;
      if (term.buffer.active.getLine(y)?.isWrapped && prev >= 0) {
        lines[prev] = `${lines[prev] ?? ''}${text}`;
      } else lines.push(text);
    }
    // The end marker sits on the line after the output, or at its end without a newline.
    if (b.end && !b.end.isDisposed) {
      const tail = lineText(b.end.line);
      const next = blocks[blocks.indexOf(b) + 1];
      if (tail.trim() && next?.prompt.line !== b.end.line) lines.push(tail);
    }
    while (lines.length > 0 && !lines[lines.length - 1]?.trim()) lines.pop();
    return lines.join('\n');
  };

  const finished = () => blocks.filter((b) => b.output && b.command);

  const at = (row: number): Block | null => {
    let found: Block | null = null;
    for (const b of blocks) {
      if (b.prompt.isDisposed) continue;
      if (b.prompt.line <= row) found = b;
      else break;
    }
    return found;
  };

  return {
    blocks: () => blocks,
    active: () => seen,
    step: (dir) => {
      const list = finished();
      if (list.length === 0) return null;
      const top = term.buffer.active.viewportY;
      const from = selected && list.includes(selected) ? selected.prompt.line : null;
      let pick: Block | undefined;
      if (dir < 0) {
        const ref = from ?? top + 1;
        pick = [...list].reverse().find((b) => b.prompt.line < ref);
      } else {
        const ref = from ?? top - 1;
        pick = list.find((b) => b.prompt.line > ref);
      }
      return pick ?? null;
    },
    reveal: (b) => {
      selected = b;
      term.scrollToLine(Math.max(0, b.prompt.line - 1));
      flash?.dispose();
      const endRow = b.end && !b.end.isDisposed ? b.end.line : b.prompt.line + 1;
      const height = Math.max(1, Math.min(endRow - b.prompt.line, term.rows));
      const colors = ev.colors();
      flash =
        term.registerDecoration({
          marker: b.prompt,
          x: 0,
          width: term.cols,
          height,
        }) ?? null;
      flash?.onRender((el) => {
        el.classList.add('vp-block-flash');
        el.style.setProperty('--vp-color', b.exit ? colors.fail : colors.ok);
      });
      const f = flash;
      setTimeout(() => {
        f?.dispose();
        if (flash === f) flash = null;
      }, 900);
    },
    last: () => {
      const list = finished();
      return list[list.length - 1] ?? null;
    },
    outputOf,
    stickyAt: () => {
      const top = term.buffer.active.viewportY;
      const b = at(top);
      if (!b?.output || !b.command || b.output.isDisposed) return null;
      // Only while the command line itself is scrolled out and output is on screen.
      if (b.prompt.line >= top || b.output.line > top) return null;
      if (b.end && !b.end.isDisposed && b.end.line <= top) return null;
      return b;
    },
    at,
    dispose: () => {
      flash?.dispose();
      inputMark?.marker.dispose();
      for (const b of blocks) drop(b);
      blocks.length = 0;
      for (const d of disposables) d.dispose();
    },
  };
}
