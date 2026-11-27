// SPDX-License-Identifier: GPL-3.0-or-later
/** Interactive prompts raised by the core during connection: host key trust and auth. */
import { ShieldAlert, ShieldQuestion } from 'lucide-react';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';

import { Button } from '@/components/ui/button';
import { Field, Input, PasswordInput } from '@/components/ui/input';
import { Dialog } from '@/components/ui/overlay';
import type { AuthPromptEvent, HostKeyEvent } from '@/lib/ipc';

export type PendingPrompt =
  { kind: 'hostKey'; event: HostKeyEvent } | { kind: 'auth'; event: AuthPromptEvent };

export function HostKeyDialog({
  event,
  onAccept,
  onReject,
}: {
  event: HostKeyEvent;
  onAccept: (remember: boolean) => void;
  onReject: () => void;
}) {
  const { t } = useTranslation();
  const changed = event.status === 'changed';
  const revoked = event.status === 'revoked';
  return (
    <Dialog
      open
      onOpenChange={(o) => {
        if (!o) onReject();
      }}
      title={
        <span className="flex items-center gap-2">
          {changed || revoked ? (
            <ShieldAlert className="text-danger size-5" />
          ) : (
            <ShieldQuestion className="text-warning size-5" />
          )}
          {revoked
            ? t('hostKey.revokedTitle')
            : changed
              ? t('hostKey.changedTitle')
              : t('hostKey.unknownTitle')}
        </span>
      }
      description={
        revoked
          ? t('hostKey.revokedDescription')
          : changed
            ? t('hostKey.changedDescription', { host: event.host })
            : t('hostKey.unknownDescription', { host: event.host })
      }
      footer={
        revoked ? (
          <Button variant="primary" onClick={onReject}>
            {t('common.close')}
          </Button>
        ) : (
          <>
            <Button variant="ghost" onClick={onReject}>
              {t('hostKey.reject')}
            </Button>
            <Button onClick={() => onAccept(false)}>{t('hostKey.acceptOnce')}</Button>
            <Button variant={changed ? 'danger' : 'primary'} onClick={() => onAccept(true)}>
              {changed ? t('hostKey.replace') : t('hostKey.trust')}
            </Button>
          </>
        )
      }
    >
      <dl className="bg-surface-2 grid grid-cols-[auto_1fr] gap-x-4 gap-y-2 rounded-xl p-4 text-[13px]">
        <dt className="text-muted">{t('hostKey.host')}</dt>
        <dd className="selectable font-mono">
          {event.host}:{event.port}
        </dd>
        <dt className="text-muted">{t('hostKey.type')}</dt>
        <dd className="font-mono">{event.keyType}</dd>
        <dt className="text-muted">{t('hostKey.fingerprint')}</dt>
        <dd className="selectable font-mono break-all">{event.fingerprint}</dd>
        {event.previousFingerprints.length > 0 ? (
          <>
            <dt className="text-muted">{t('hostKey.previous')}</dt>
            <dd className="selectable text-danger font-mono break-all">
              {event.previousFingerprints.join('\n')}
            </dd>
          </>
        ) : null}
      </dl>
      {event.randomart ? (
        <pre className="bg-surface-2 text-muted mt-3 overflow-x-auto rounded-xl p-3 font-mono text-[11px] leading-tight">
          {event.randomart}
        </pre>
      ) : null}
      {changed ? <p className="text-danger mt-3 text-[13px]">{t('hostKey.mitmWarning')}</p> : null}
    </Dialog>
  );
}

export function AuthPromptDialog({
  event,
  onSubmit,
  onCancel,
}: {
  event: AuthPromptEvent;
  onSubmit: (answers: string[]) => void;
  onCancel: () => void;
}) {
  const { t } = useTranslation();
  const [answers, setAnswers] = useState<string[]>(() => event.prompts.map(() => ''));
  return (
    <Dialog
      open
      onOpenChange={(o) => {
        if (!o) onCancel();
      }}
      title={event.name || t('auth.title', { target: event.target })}
      description={event.instructions || undefined}
      footer={
        <>
          <Button variant="ghost" onClick={onCancel}>
            {t('common.cancel')}
          </Button>
          <Button variant="primary" onClick={() => onSubmit(answers)}>
            {t('auth.continue')}
          </Button>
        </>
      }
    >
      <form
        className="grid gap-3"
        onSubmit={(e) => {
          e.preventDefault();
          onSubmit(answers);
        }}
      >
        {event.prompts.map((p, i) => (
          <Field key={`${p.text}-${String(i)}`} label={p.text}>
            {(id) =>
              p.echo ? (
                <Input
                  id={id}
                  autoFocus={i === 0}
                  value={answers[i] ?? ''}
                  onChange={(e) =>
                    setAnswers((a) => a.map((x, j) => (j === i ? e.target.value : x)))
                  }
                />
              ) : (
                <PasswordInput
                  id={id}
                  autoFocus={i === 0}
                  value={answers[i] ?? ''}
                  onChange={(e) =>
                    setAnswers((a) => a.map((x, j) => (j === i ? e.target.value : x)))
                  }
                />
              )
            }
          </Field>
        ))}
        <button type="submit" className="hidden" aria-hidden tabIndex={-1} />
      </form>
    </Dialog>
  );
}
