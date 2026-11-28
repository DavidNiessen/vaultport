// SPDX-License-Identifier: GPL-3.0-or-later
import {
  Braces,
  CornerDownRight,
  File,
  Flag,
  Folder,
  History,
  ListTree,
  SquareTerminal,
  Variable,
  type LucideIcon,
} from 'lucide-react';
import { useId } from 'react';
import { useTranslation } from 'react-i18next';

import { cn } from '@/lib/cn';
import type { CompletionItem } from '@/lib/ipc';

import type { PopupView } from './controller';
import { highlightRuns, matchIndexes, queryFor } from './match';

function iconFor(item: CompletionItem): LucideIcon {
  switch (item.kind) {
    case 'path':
      return item.label.endsWith('/') ? Folder : File;
    case 'command':
      return SquareTerminal;
    case 'subcommand':
      return ListTree;
    case 'option':
      return Flag;
    case 'snippet':
      return Braces;
    case 'history':
      return History;
    case 'value':
      return item.label.startsWith('$') ? Variable : CornerDownRight;
  }
}

const ICON_COLOR: Record<CompletionItem['kind'], string> = {
  path: 'text-info',
  command: 'text-success',
  subcommand: 'text-accent-text',
  option: 'text-warning',
  snippet: 'text-accent-text',
  history: 'text-subtle',
  value: 'text-info',
};

/**
 * The suggestion list at the terminal cursor. Mouse presses do not take focus from the
 * terminal, so typing continues while it is open.
 */
export function AutocompletePopup({
  view,
  onPick,
  onHover,
}: {
  view: PopupView;
  onPick: (index: number) => void;
  onHover: (index: number) => void;
}) {
  const { t } = useTranslation();
  // Each terminal pane has its own popup, so option ids need a per-instance prefix.
  const base = useId();
  const active = `${base}-${view.selected}`;
  const selected = view.items[view.selected];
  return (
    <div
      role="listbox"
      aria-label={t('terminal.autocomplete.label')}
      aria-activedescendant={active}
      tabIndex={-1}
      data-testid="autocomplete"
      className={cn(
        'bg-surface-2/95 border-border animate-fade-in absolute z-20 w-[min(28rem,calc(100%-1rem))] overflow-hidden rounded-xl border py-1 shadow-[var(--shadow)] backdrop-blur-md',
        view.above && '-translate-y-full',
      )}
      style={{ left: view.left, top: view.top }}
      onMouseDown={(e) => e.preventDefault()}
    >
      {view.items.map((item, i) => {
        const Icon = iconFor(item);
        const runs = highlightRuns(
          item.label,
          matchIndexes(item.label, queryFor(view.word, item.kind)),
        );
        return (
          <div
            key={`${item.kind}:${item.text}`}
            id={`${base}-${i}`}
            role="option"
            aria-selected={i === view.selected}
            tabIndex={-1}
            className={cn(
              'flex h-[30px] items-center gap-2 px-2.5 text-[13px]',
              i === view.selected ? 'bg-accent-soft text-text' : 'text-muted',
            )}
            onMouseMove={() => onHover(i)}
            onClick={() => onPick(i)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') onPick(i);
            }}
          >
            <Icon className={cn('size-3.5 shrink-0', ICON_COLOR[item.kind])} aria-hidden />
            <span className="text-text min-w-0 shrink truncate font-mono">
              {runs.map((r, j) =>
                r.hit ? (
                  <mark key={j} className="text-accent-text bg-transparent font-bold">
                    {r.text}
                  </mark>
                ) : (
                  <span key={j}>{r.text}</span>
                ),
              )}
            </span>
            {item.description ? (
              <span className="text-subtle min-w-0 flex-1 truncate text-right text-[12px]">
                {item.description}
              </span>
            ) : (
              <span className="flex-1" />
            )}
            <span className="text-subtle bg-surface-3 shrink-0 rounded px-1.5 py-px text-[11px]">
              {t(`terminal.autocomplete.kind.${item.kind}`)}
            </span>
          </div>
        );
      })}
      {selected && (selected.description || selected.text !== selected.label) ? (
        <div className="border-border mt-1 border-t px-2.5 pt-1.5 pb-0.5">
          {selected.text !== selected.label ? (
            <div className="text-muted truncate font-mono text-[11px]">{selected.text}</div>
          ) : null}
          {selected.description ? (
            <div className="text-muted line-clamp-2 text-[12px]">{selected.description}</div>
          ) : null}
        </div>
      ) : null}
      <div className="text-subtle border-border mt-1 border-t px-2.5 pt-1 text-[11px]">
        {t('terminal.autocomplete.hint')}
      </div>
    </div>
  );
}

/** The rest of the suggested command, drawn dimmed after the cursor. */
export function GhostText({
  text,
  left,
  top,
  height,
  fontFamily,
  fontSize,
  color,
}: {
  text: string;
  left: number;
  top: number;
  height: number;
  fontFamily: string;
  fontSize: number;
  color: string;
}) {
  return (
    <div
      aria-hidden
      data-testid="ghost-text"
      className="pointer-events-none absolute z-10 overflow-hidden whitespace-pre"
      style={{
        left,
        top,
        height,
        lineHeight: `${height}px`,
        fontFamily,
        fontSize,
        color,
        opacity: 0.45,
        maxWidth: `calc(100% - ${left}px)`,
      }}
    >
      {text}
    </div>
  );
}
