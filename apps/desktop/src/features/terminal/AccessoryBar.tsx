// SPDX-License-Identifier: GPL-3.0-or-later
/**
 * Touch keyboard accessory bar shown above the on-screen keyboard on phones and tablets.
 * Ctrl and Alt are sticky modifiers applied to the next key typed on the bar.
 */
import { ArrowDown, ArrowLeft, ArrowRight, ArrowUp, Minus, Plus } from 'lucide-react';
import { useState, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';

import { cn } from '@/lib/cn';

const ESC = '\x1b';

interface Key {
  label: ReactNode;
  aria: string;
  seq: string;
  wide?: boolean;
}

const ARROWS: Key[] = [
  { label: <ArrowUp className="size-4" />, aria: 'Up', seq: `${ESC}[A` },
  { label: <ArrowDown className="size-4" />, aria: 'Down', seq: `${ESC}[B` },
  { label: <ArrowLeft className="size-4" />, aria: 'Left', seq: `${ESC}[D` },
  { label: <ArrowRight className="size-4" />, aria: 'Right', seq: `${ESC}[C` },
];

const SYMBOLS: Key[] = [
  '|',
  '/',
  '-',
  '~',
  '_',
  '*',
  '&',
  '$',
  '>',
  '<',
  ':',
  '"',
  "'",
  '`',
  '{',
  '}',
  '[',
  ']',
].map((s) => ({ label: s, aria: s, seq: s }));

const FKEYS: Key[] = [
  ['F1', `${ESC}OP`],
  ['F2', `${ESC}OQ`],
  ['F3', `${ESC}OR`],
  ['F4', `${ESC}OS`],
  ['F5', `${ESC}[15~`],
  ['F6', `${ESC}[17~`],
  ['F7', `${ESC}[18~`],
  ['F8', `${ESC}[19~`],
  ['F9', `${ESC}[20~`],
  ['F10', `${ESC}[21~`],
  ['PgUp', `${ESC}[5~`],
  ['PgDn', `${ESC}[6~`],
  ['Home', `${ESC}[H`],
  ['End', `${ESC}[F`],
].map(([l, s]) => ({ label: l, aria: l ?? '', seq: s ?? '', wide: true }));

/** Applies a sticky Ctrl/Alt modifier to a single-character key. */
export function applyModifiers(seq: string, ctrl: boolean, alt: boolean): string {
  let out = seq;
  if (ctrl && out.length === 1) {
    const c = out.toUpperCase().charCodeAt(0);
    if (c >= 64 && c <= 95) out = String.fromCharCode(c - 64);
    else if (out === ' ') out = '\x00';
  }
  if (alt) out = ESC + out;
  return out;
}

export function AccessoryBar({
  onKey,
  onFontSize,
}: {
  onKey: (seq: string) => void;
  onFontSize: (delta: number) => void;
}) {
  const { t } = useTranslation();
  const [ctrl, setCtrl] = useState(false);
  const [alt, setAlt] = useState(false);
  const [page, setPage] = useState<'main' | 'symbols' | 'fn'>('main');

  const press = (seq: string) => {
    onKey(applyModifiers(seq, ctrl, alt));
    setCtrl(false);
    setAlt(false);
  };

  const btn = (k: Key) => (
    <button
      key={k.aria}
      type="button"
      aria-label={k.aria}
      onPointerDown={(e) => {
        e.preventDefault();
        press(k.seq);
      }}
      className={cn(
        'bg-surface-3 text-text active:bg-accent active:text-accent-fg flex h-9 shrink-0 items-center justify-center rounded-lg font-mono text-[13px]',
        k.wide ? 'min-w-12 px-2' : 'w-10',
      )}
    >
      {k.label}
    </button>
  );

  const toggle = (label: string, on: boolean, set: (v: boolean) => void) => (
    <button
      type="button"
      aria-pressed={on}
      onPointerDown={(e) => {
        e.preventDefault();
        set(!on);
      }}
      className={cn(
        'flex h-9 min-w-12 shrink-0 items-center justify-center rounded-lg px-2 text-[13px] font-medium',
        on ? 'bg-accent text-accent-fg' : 'bg-surface-3 text-text',
      )}
    >
      {label}
    </button>
  );

  return (
    <div
      role="toolbar"
      aria-label={t('terminal.keyboardBar')}
      className="border-border bg-surface-1 safe-bottom flex [scrollbar-width:none] items-center gap-1.5 overflow-x-auto border-t px-2 py-1.5"
    >
      {btn({ label: 'esc', aria: 'Escape', seq: ESC, wide: true })}
      {btn({ label: 'tab', aria: 'Tab', seq: '\t', wide: true })}
      {toggle('ctrl', ctrl, setCtrl)}
      {toggle('alt', alt, setAlt)}
      {page === 'main' && ARROWS.map(btn)}
      {page === 'symbols' && SYMBOLS.map(btn)}
      {page === 'fn' && FKEYS.map(btn)}
      <button
        type="button"
        onPointerDown={(e) => {
          e.preventDefault();
          setPage(page === 'main' ? 'symbols' : page === 'symbols' ? 'fn' : 'main');
        }}
        className="bg-surface-4 text-muted flex h-9 min-w-12 shrink-0 items-center justify-center rounded-lg px-2 text-[12px]"
        aria-label={t('terminal.moreKeys')}
      >
        {page === 'main' ? '#+=' : page === 'symbols' ? 'Fn' : '←→'}
      </button>
      <div className="flex-1" />
      <button
        type="button"
        aria-label={t('terminal.fontSmaller')}
        onPointerDown={(e) => {
          e.preventDefault();
          onFontSize(-1);
        }}
        className="bg-surface-3 flex size-9 shrink-0 items-center justify-center rounded-lg"
      >
        <Minus className="size-4" />
      </button>
      <button
        type="button"
        aria-label={t('terminal.fontLarger')}
        onPointerDown={(e) => {
          e.preventDefault();
          onFontSize(1);
        }}
        className="bg-surface-3 flex size-9 shrink-0 items-center justify-center rounded-lg"
      >
        <Plus className="size-4" />
      </button>
    </div>
  );
}
