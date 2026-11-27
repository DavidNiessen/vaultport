// SPDX-License-Identifier: GPL-3.0-or-later
/**
 * Approval dialog for actions requested by AI agents over MCP, and the "open a terminal for
 * this host" hand-over. Requests queue up; closing the dialog denies the current one.
 */
import { listen } from '@tauri-apps/api/event';
import { Bot, TriangleAlert } from 'lucide-react';
import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';

import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/controls';
import { Dialog } from '@/components/ui/overlay';
import { commands, errorMessage, type AgentApprovalRequest, type AgentDecision } from '@/lib/ipc';

import { useSessions } from '../terminal/sessions';
import { APPROVAL_DONE_EVENT, APPROVAL_EVENT, OPEN_TERMINAL_EVENT } from './events';

export function AgentApprovals() {
  const { t } = useTranslation();
  const sessions = useSessions();
  const [queue, setQueue] = useState<AgentApprovalRequest[]>([]);

  useEffect(() => {
    const unlisten: (() => void)[] = [];
    let cancelled = false;
    const keep = (u: () => void) => (cancelled ? u() : unlisten.push(u));
    void listen<AgentApprovalRequest>(APPROVAL_EVENT, (e) => {
      setQueue((q) => [...q, e.payload]);
    }).then(keep);
    void listen<string>(APPROVAL_DONE_EVENT, (e) => {
      setQueue((q) => q.filter((r) => r.id !== e.payload));
    }).then(keep);
    void listen<string>(OPEN_TERMINAL_EVENT, (e) => {
      commands
        .listHosts()
        .then((hosts) => {
          const h = hosts.find((x) => x.id === e.payload);
          if (h) sessions.connectHost(h);
        })
        .catch((err: unknown) => toast.error(errorMessage(err)));
    }).then(keep);
    return () => {
      cancelled = true;
      unlisten.forEach((u) => {
        u();
      });
    };
  }, [sessions]);

  const current = queue[0];
  if (!current) return null;

  const answer = (decision: AgentDecision) => {
    setQueue((q) => q.filter((r) => r.id !== current.id));
    commands.agentRespond(current.id, decision).catch(() => {
      // Already answered or timed out; nothing to do.
    });
  };

  return (
    <Dialog
      open
      onOpenChange={(o) => !o && answer('deny')}
      title={t('agents.approval.title', { client: current.client })}
      description={t('agents.approval.description')}
      footer={
        <>
          <Button variant="ghost" onClick={() => answer('deny')}>
            {t('agents.approval.deny')}
          </Button>
          <Button onClick={() => answer('allowSession')}>{t('agents.approval.session')}</Button>
          <Button
            variant={current.destructive ? 'danger' : 'primary'}
            onClick={() => answer('allowOnce')}
            autoFocus
          >
            {t('agents.approval.once')}
          </Button>
        </>
      }
    >
      <div className="grid gap-3" data-testid="agent-approval">
        <div className="flex items-center gap-3">
          <div className="bg-accent-soft text-accent-text flex size-9 shrink-0 items-center justify-center rounded-xl">
            <Bot className="size-5" />
          </div>
          <div className="min-w-0">
            <div className="text-[13px] font-semibold">{current.title}</div>
            {current.host ? (
              <div className="text-muted truncate text-xs">
                {t('agents.approval.on', { host: current.host })}
              </div>
            ) : null}
          </div>
          {current.destructive ? (
            <Badge tone="warning" className="ml-auto gap-1">
              <TriangleAlert className="size-3" />
              {t('agents.approval.changes')}
            </Badge>
          ) : null}
        </div>
        {current.details ? (
          <pre className="bg-surface-2 max-h-72 overflow-auto rounded-lg p-3 font-mono text-xs break-all whitespace-pre-wrap select-text">
            {current.details}
          </pre>
        ) : null}
        <p className="text-subtle text-xs">{t('agents.approval.sessionHint')}</p>
        {queue.length > 1 ? (
          <p className="text-subtle text-xs">
            {t('agents.approval.more', { count: queue.length - 1 })}
          </p>
        ) : null}
      </div>
    </Dialog>
  );
}
