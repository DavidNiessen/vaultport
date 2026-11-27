// SPDX-License-Identifier: GPL-3.0-or-later
import { useId } from 'react';
import { useTranslation } from 'react-i18next';

import { cn } from '@/lib/cn';

import type { PopupView } from './controller';

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
  return (
    <div
      role="listbox"
      aria-label={t('terminal.autocomplete.label')}
      aria-activedescendant={active}
      tabIndex={-1}
      data-testid="autocomplete"
      className={cn(
        'bg-surface-2 border-border absolute z-20 w-[min(26rem,calc(100%-1rem))] overflow-hidden rounded-xl border py-1 shadow-[var(--shadow)]',
        view.above && '-translate-y-full',
      )}
      style={{ left: view.left, top: view.top }}
      onMouseDown={(e) => e.preventDefault()}
    >
      {view.items.map((item, i) => (
        <div
          key={`${item.kind}:${item.text}`}
          id={`${base}-${i}`}
          role="option"
          aria-selected={i === view.selected}
          tabIndex={-1}
          className={cn(
            'flex h-[30px] cursor-default items-center gap-2 px-2.5 text-[13px]',
            i === view.selected ? 'bg-accent-soft text-text' : 'text-muted',
          )}
          onMouseMove={() => onHover(i)}
          onClick={() => onPick(i)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') onPick(i);
          }}
        >
          <span className="text-text min-w-0 shrink truncate font-mono">{item.label}</span>
          {item.description ? (
            <span className="text-subtle min-w-0 flex-1 truncate">{item.description}</span>
          ) : (
            <span className="flex-1" />
          )}
          <span className="text-subtle bg-surface-3 shrink-0 rounded px-1.5 py-px text-[11px]">
            {t(`terminal.autocomplete.kind.${item.kind}`)}
          </span>
        </div>
      ))}
      <div className="text-subtle border-border mt-1 border-t px-2.5 pt-1 text-[11px]">
        {t('terminal.autocomplete.hint')}
      </div>
    </div>
  );
}
