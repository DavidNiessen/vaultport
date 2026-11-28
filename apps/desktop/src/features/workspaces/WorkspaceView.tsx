// SPDX-License-Identifier: GPL-3.0-or-later
/** Workspaces: several sessions in one tab, as a split grid or focus mode, with broadcast. */
import { Columns2, Maximize2, Radio, Save, SquarePlus, SquareTerminal, X } from 'lucide-react';
import { useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';

import { Button } from '@/components/ui/button';
import { Tooltip } from '@/components/ui/controls';
import { cn } from '@/lib/cn';
import { useBreakpoint } from '@/lib/platform';
import { useSaveWorkspace } from '@/lib/queries';
import { useApp, type TerminalTab, type WorkspaceTab } from '@/stores/app';

import { terminalRegistry } from '../terminal/registry';
import { splitPane } from '../terminal/tabActions';
import { TerminalSlot } from '../terminal/TerminalLayer';

/** Grid dimensions for `n` panes (max 16), preferring wide layouts. */
export function gridFor(n: number): { cols: number; rows: number } {
  const count = Math.max(1, Math.min(16, n));
  const cols = Math.ceil(Math.sqrt(count));
  return { cols, rows: Math.ceil(count / cols) };
}

const dot: Record<TerminalTab['status'], string> = {
  connecting: 'bg-warning',
  connected: 'bg-success',
  disconnected: 'bg-subtle',
  error: 'bg-danger',
};

export function WorkspaceView({ ws, active }: { ws: WorkspaceTab; active: boolean }) {
  const { t } = useTranslation();
  const bp = useBreakpoint();
  const tabs = useApp((s) => s.tabs);
  const updateTab = useApp((s) => s.updateTab);
  const saveWs = useSaveWorkspace();
  const panes = ws.panes
    .map((id) => tabs.find((x) => x.id === id))
    .filter((x): x is TerminalTab => x?.kind === 'terminal');
  const maxPanes = bp === 'tablet' ? 4 : 16;
  const mode = bp === 'phone' ? 'focus' : ws.mode;
  const focused = panes.find((p) => p.id === ws.focused) ?? panes[0];
  const visible = mode === 'split' ? panes.slice(0, maxPanes) : panes;
  const { cols, rows } = gridFor(visible.length);

  // Keyboard focus follows the focused pane (e.g. after splitting or cycling panes).
  useEffect(() => {
    if (!active || !ws.focused) return;
    const id = ws.focused;
    requestAnimationFrame(() => terminalRegistry.get(id)?.focus());
  }, [active, ws.focused]);

  const saveTemplate = () => {
    saveWs.mutate(
      {
        id: ws.templateId ?? ws.id,
        label: ws.title,
        mode: ws.mode,
        panes: panes
          .filter((p) => p.hostId)
          .map((p) => ({ host: p.hostId ?? '', title: p.title, cwd: null, command: null })),
        columnSizes: [],
        createdAt: 0,
        updatedAt: 0,
      },
      {
        onSuccess: (saved) => {
          updateTab(ws.id, { templateId: saved.id });
          toast.success(t('workspaces.saved'));
        },
      },
    );
  };

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="border-border flex h-10 shrink-0 items-center gap-1 border-b px-2">
        <span className="text-muted px-2 text-[13px] font-medium">{ws.title}</span>
        <div className="flex-1" />
        <Tooltip content={t('workspaces.broadcastHint')}>
          <Button
            size="sm"
            variant={ws.broadcast ? 'danger' : 'ghost'}
            aria-pressed={ws.broadcast}
            onClick={() => updateTab(ws.id, { broadcast: !ws.broadcast })}
          >
            <Radio className="size-4" />
            {t('workspaces.broadcast')}
          </Button>
        </Tooltip>
        {bp !== 'phone' ? (
          <Button
            size="sm"
            variant="ghost"
            onClick={() => updateTab(ws.id, { mode: ws.mode === 'split' ? 'focus' : 'split' })}
          >
            {ws.mode === 'split' ? (
              <Maximize2 className="size-4" />
            ) : (
              <Columns2 className="size-4" />
            )}
            {ws.mode === 'split' ? t('workspaces.focusMode') : t('workspaces.splitView')}
          </Button>
        ) : null}
        <Tooltip content={t('shortcuts.splitPane')}>
          <Button
            size="sm"
            variant="ghost"
            aria-label={t('shortcuts.splitPane')}
            onClick={() => splitPane(ws.id)}
          >
            <SquarePlus className="size-4" />
          </Button>
        </Tooltip>
        <Button size="sm" variant="ghost" onClick={saveTemplate}>
          <Save className="size-4" />
          {t('workspaces.saveTemplate')}
        </Button>
      </div>
      <div className="flex min-h-0 flex-1">
        {mode === 'focus' ? (
          <aside className="border-border hidden w-48 shrink-0 flex-col gap-0.5 overflow-y-auto border-r p-2 sm:flex">
            {panes.map((p) => (
              <button
                key={p.id}
                type="button"
                onClick={() => updateTab(ws.id, { focused: p.id })}
                className={cn(
                  'flex h-9 items-center gap-2 rounded-lg px-2 text-left text-[13px]',
                  p.id === focused?.id ? 'bg-surface-3' : 'hover:bg-surface-2 text-muted',
                )}
              >
                <span
                  className={cn('size-1.5 rounded-full', p.unread ? 'bg-info' : dot[p.status])}
                />
                <span className="truncate">{p.title}</span>
              </button>
            ))}
          </aside>
        ) : null}
        <div
          className={cn(
            'min-h-0 min-w-0 flex-1',
            mode === 'split' && 'grid gap-px bg-[var(--border)]',
          )}
          style={
            mode === 'split'
              ? {
                  gridTemplateColumns: `repeat(${String(cols)}, minmax(0, 1fr))`,
                  gridTemplateRows: `repeat(${String(rows)}, minmax(0, 1fr))`,
                }
              : undefined
          }
        >
          {visible.map((p) => {
            const show = mode === 'split' || p.id === focused?.id;
            return (
              <div
                key={p.id}
                className={cn(
                  'relative flex min-h-0 min-w-0 flex-col',
                  !show && 'hidden',
                  mode === 'split' &&
                    p.id === focused?.id &&
                    ws.broadcast &&
                    'ring-danger ring-2 ring-inset',
                  mode === 'focus' && 'h-full',
                )}
                onFocusCapture={() => {
                  if (ws.focused !== p.id) updateTab(ws.id, { focused: p.id });
                }}
              >
                {mode === 'split' ? (
                  <div
                    className={cn(
                      'bg-surface-1 flex h-7 shrink-0 items-center gap-2 px-2.5 text-[12px]',
                      p.id === focused?.id ? 'text-text' : 'text-muted',
                    )}
                  >
                    <SquareTerminal className="size-3.5" />
                    <span className="text-success truncate font-medium">{p.title}</span>
                    <span
                      className={cn(
                        'ml-auto size-1.5 rounded-full',
                        p.unread ? 'bg-info' : dot[p.status],
                      )}
                      aria-label={p.status}
                    />
                    <button
                      type="button"
                      aria-label={t('tabs.closePane', { title: p.title })}
                      onClick={() => useApp.getState().closePane(ws.id, p.id)}
                      className="text-subtle hover:text-text hover:bg-text/[0.1] flex size-5 items-center justify-center rounded"
                    >
                      <X className="size-3.5" />
                    </button>
                  </div>
                ) : null}
                <TerminalSlot id={p.id} visible={active && show} className="min-h-0 flex-1" />
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}
