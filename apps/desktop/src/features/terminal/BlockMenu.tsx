// SPDX-License-Identifier: GPL-3.0-or-later
/** Actions for one command block, opened from its exit-status badge. */
import { Copy, CopyPlus, FolderOpen, RotateCw, Sparkles, Wand2, X } from 'lucide-react';
import { useEffect, useRef, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';

import { cn } from '@/lib/cn';

import { formatDuration, type Block } from './blocks';

export interface BlockMenuView {
  block: Block;
  /** Position relative to the terminal root element. */
  left: number;
  top: number;
}

function Item({ icon, label, onClick }: { icon: ReactNode; label: string; onClick: () => void }) {
  return (
    <button
      type="button"
      role="menuitem"
      onClick={onClick}
      className="text-text hover:bg-surface-3 focus:bg-surface-3 [&>svg]:text-muted flex h-8 w-full items-center gap-2.5 rounded-lg px-2.5 text-left text-[13px] outline-none [&>svg]:size-4"
    >
      {icon}
      {label}
    </button>
  );
}

export function BlockMenu({
  view,
  aiEnabled,
  agentsEnabled,
  onCopyCommand,
  onCopyOutput,
  onCopyBoth,
  onRerun,
  onFix,
  onExplain,
  onClose,
}: {
  view: BlockMenuView;
  aiEnabled: boolean;
  agentsEnabled: boolean;
  onCopyCommand: () => void;
  onCopyOutput: () => void;
  onCopyBoth: () => void;
  onRerun: () => void;
  onFix: () => void;
  onExplain: () => void;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const ref = useRef<HTMLDivElement>(null);
  const b = view.block;
  const failed = b.exit !== null && b.exit !== 0;
  const ms = b.startedAt !== null && b.finishedAt !== null ? b.finishedAt - b.startedAt : null;

  useEffect(() => {
    ref.current?.querySelector<HTMLElement>('[role="menuitem"]')?.focus();
    const onDown = (e: MouseEvent) => {
      if (!ref.current?.contains(e.target as Node)) onClose();
    };
    window.addEventListener('mousedown', onDown);
    return () => window.removeEventListener('mousedown', onDown);
  }, [onClose]);

  const run = (fn: () => void) => () => {
    fn();
    onClose();
  };

  return (
    <div
      ref={ref}
      role="menu"
      aria-label={t('terminal.block.menu')}
      data-testid="block-menu"
      tabIndex={-1}
      onKeyDown={(e) => {
        if (e.key === 'Escape') onClose();
        if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
          e.preventDefault();
          const items = [
            ...(ref.current?.querySelectorAll<HTMLElement>('[role="menuitem"]') ?? []),
          ];
          const i = items.indexOf(document.activeElement as HTMLElement);
          items[(i + (e.key === 'ArrowDown' ? 1 : items.length - 1)) % items.length]?.focus();
        }
      }}
      className="bg-surface-2 border-border animate-fade-in absolute z-30 w-72 -translate-x-full rounded-xl border p-1 shadow-[var(--shadow)]"
      style={{ left: view.left, top: view.top }}
    >
      <div className="border-border mb-1 border-b px-2.5 pt-1.5 pb-2">
        <div className="flex items-center gap-2">
          <code className="text-text min-w-0 flex-1 truncate font-mono text-[12px]">
            {b.command}
          </code>
          <button
            type="button"
            aria-label={t('common.close')}
            onClick={onClose}
            className="text-subtle hover:text-text"
          >
            <X className="size-3.5" />
          </button>
        </div>
        <div className="text-subtle mt-1 flex flex-wrap items-center gap-x-2 text-[11px]">
          <span className={cn('font-semibold', failed ? 'text-danger' : 'text-success')}>
            {b.exit === null
              ? t('terminal.block.noStatus')
              : t('terminal.block.exit', { code: b.exit })}
          </span>
          {ms !== null ? <span>{formatDuration(ms)}</span> : null}
          {b.finishedAt ? <span>{new Date(b.finishedAt).toLocaleTimeString()}</span> : null}
          {b.cwd ? (
            <span className="flex min-w-0 items-center gap-1 truncate">
              <FolderOpen className="size-3" aria-hidden />
              {b.cwd}
            </span>
          ) : null}
        </div>
      </div>
      <Item icon={<Copy />} label={t('terminal.block.copyCommand')} onClick={run(onCopyCommand)} />
      <Item icon={<Copy />} label={t('terminal.block.copyOutput')} onClick={run(onCopyOutput)} />
      <Item icon={<CopyPlus />} label={t('terminal.block.copyBoth')} onClick={run(onCopyBoth)} />
      <Item icon={<RotateCw />} label={t('terminal.block.rerun')} onClick={run(onRerun)} />
      {aiEnabled && failed ? (
        <Item icon={<Wand2 />} label={t('terminal.block.fix')} onClick={run(onFix)} />
      ) : null}
      {agentsEnabled ? (
        <Item icon={<Sparkles />} label={t('terminal.block.explain')} onClick={run(onExplain)} />
      ) : null}
    </div>
  );
}
