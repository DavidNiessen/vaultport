// SPDX-License-Identifier: GPL-3.0-or-later
/**
 * The terminal's context bar: working directory, git branch and how the last command went.
 * Shown when the shell reports its directory (shell integration).
 */
import { writeText } from '@tauri-apps/plugin-clipboard-manager';
import { CircleCheck, CircleX, FolderOpen, GitBranch } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';

import { cn } from '@/lib/cn';

import { formatDuration } from './blocks';

export interface TermContextView {
  cwd: string;
  gitHead: string | null;
  last: { exit: number | null; ms: number | null } | null;
}

/** `/home/me/src` → `~/src` (also `/Users/me` and `/root`). */
export function shortenHome(path: string): string {
  return path.replace(/^(\/home\/[^/]+|\/Users\/[^/]+|\/root)(?=\/|$)/, '~');
}

export function ContextBar({ view }: { view: TermContextView | null }) {
  const { t } = useTranslation();
  if (!view) return <div className="h-6 shrink-0" aria-hidden />;
  const failed = view.last?.exit !== null && view.last?.exit !== undefined && view.last.exit !== 0;
  return (
    <div
      data-testid="context-bar"
      className="border-border/60 text-muted flex h-6 shrink-0 items-center gap-3 border-t px-3 text-[11.5px]"
    >
      <button
        type="button"
        title={t('terminal.context.copyPath')}
        onClick={() =>
          void writeText(view.cwd).then(() => toast.success(t('terminal.block.copied')))
        }
        className="hover:text-text flex min-w-0 items-center gap-1.5 font-mono"
      >
        <FolderOpen className="size-3.5 shrink-0" aria-hidden />
        <span className="truncate">{shortenHome(view.cwd)}</span>
      </button>
      {view.gitHead ? (
        <span className="text-accent-text flex min-w-0 items-center gap-1 font-mono">
          <GitBranch className="size-3.5 shrink-0" aria-hidden />
          <span className="truncate">{view.gitHead}</span>
        </span>
      ) : null}
      <span className="flex-1" />
      {view.last ? (
        <span
          className={cn(
            'flex shrink-0 items-center gap-1',
            failed ? 'text-danger' : 'text-success',
          )}
        >
          {failed ? (
            <CircleX className="size-3.5" aria-hidden />
          ) : (
            <CircleCheck className="size-3.5" aria-hidden />
          )}
          {failed ? t('terminal.block.exit', { code: view.last.exit }) : t('terminal.context.ok')}
          {view.last.ms !== null ? (
            <span className="text-subtle">· {formatDuration(view.last.ms)}</span>
          ) : null}
        </span>
      ) : null}
    </div>
  );
}
