// SPDX-License-Identifier: GPL-3.0-or-later
/**
 * Prompts and notifications raised by background work that is not tied to a terminal tab:
 * host-key and authentication prompts for SFTP and port forwarding, and "file edited
 * locally — upload?" offers from the SFTP editor.
 */
import { listen } from '@tauri-apps/api/event';
import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';

import { commands, errorMessage, type SessionEvent } from '@/lib/ipc';

import { AuthPromptDialog, HostKeyDialog, type PendingPrompt } from '../features/terminal/Prompts';

export function GlobalPrompts() {
  const { t } = useTranslation();
  const [queue, setQueue] = useState<PendingPrompt[]>([]);

  useEffect(() => {
    const unlisten: (() => void)[] = [];
    let cancelled = false;
    void listen<{ event: SessionEvent }>('prompt://request', (e) => {
      const ev = e.payload.event;
      if (ev.type === 'hostKey') setQueue((q) => [...q, { kind: 'hostKey', event: ev }]);
      else if (ev.type === 'auth') setQueue((q) => [...q, { kind: 'auth', event: ev }]);
    }).then((u) => (cancelled ? u() : unlisten.push(u)));
    void listen<{ editId: string; name: string }>('sftp://edited', (e) => {
      const { editId, name } = e.payload;
      toast(t('sftp.editedLocally', { name }), {
        duration: 60_000,
        action: {
          label: t('sftp.upload'),
          onClick: () => {
            commands
              .fsUploadEdit(editId, false)
              .then(() => toast.success(t('sftp.uploaded', { name })))
              .catch((err: unknown) => toast.error(errorMessage(err)));
          },
        },
      });
    }).then((u) => (cancelled ? u() : unlisten.push(u)));
    return () => {
      cancelled = true;
      unlisten.forEach((u) => {
        u();
      });
    };
  }, [t]);

  const current = queue[0];
  if (!current) return null;
  const done = () => setQueue((q) => q.slice(1));

  if (current.kind === 'hostKey') {
    const respond = (accept: boolean, remember: boolean) => {
      void commands.promptRespond(current.event.promptId, { type: 'hostKey', accept, remember });
      done();
    };
    return (
      <HostKeyDialog
        event={current.event}
        onAccept={(remember) => respond(true, remember)}
        onReject={() => respond(false, false)}
      />
    );
  }
  return (
    <AuthPromptDialog
      event={current.event}
      onSubmit={(answers) => {
        void commands.promptRespond(current.event.promptId, { type: 'auth', answers });
        done();
      }}
      onCancel={() => {
        void commands.promptRespond(current.event.promptId, { type: 'auth', answers: null });
        done();
      }}
    />
  );
}
