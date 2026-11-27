// SPDX-License-Identifier: GPL-3.0-or-later
/** First-run vault creation and unlock screen. */
import { Fingerprint, KeyRound, Lock, ShieldCheck } from 'lucide-react';
import { useState, type SubmitEvent } from 'react';
import { useTranslation } from 'react-i18next';

import { Button } from '@/components/ui/button';
import { Select, Switch } from '@/components/ui/controls';
import { Field, Input, PasswordInput } from '@/components/ui/input';
import { commands, errorMessage, type VaultStatus } from '@/lib/ipc';
import { usePlatform } from '@/lib/platform';

/** Rough password strength estimate (0-4) for user feedback only. */
export function passwordStrength(pw: string): number {
  if (pw.length < 8) return 0;
  let score = 0;
  if (pw.length >= 12) score++;
  if (pw.length >= 16) score++;
  if (/[a-z]/.test(pw) && /[A-Z]/.test(pw)) score++;
  if (/\d/.test(pw)) score++;
  if (/[^A-Za-z0-9]/.test(pw)) score++;
  return Math.min(4, score);
}

function Brand() {
  const { t } = useTranslation();
  return (
    <div className="mb-8 flex flex-col items-center text-center">
      <img src="/logo.svg" alt="" className="mb-4 size-16" />
      <h1 className="text-2xl font-semibold tracking-tight">ssh-client</h1>
      <p className="text-muted mt-1 text-[13px]">{t('lock.tagline')}</p>
    </div>
  );
}

export function LockScreen({
  status,
  onChange,
}: {
  status: VaultStatus;
  onChange: (s: VaultStatus) => void;
}) {
  const { t } = useTranslation();
  const platform = usePlatform();
  const creating = status.vaults.length === 0;
  const [mode, setMode] = useState<'unlock' | 'create'>(creating ? 'create' : 'unlock');
  const [name, setName] = useState(status.active ?? status.vaults[0] ?? 'personal');
  const [password, setPassword] = useState('');
  const [confirm, setConfirm] = useState('');
  const [remember, setRemember] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const strength = passwordStrength(password);
  const canCreate =
    /^[A-Za-z0-9_-]{1,64}$/.test(name) &&
    (password.length === 0 ? remember : password.length >= 8 && password === confirm);

  const submit = async (e: SubmitEvent) => {
    e.preventDefault();
    setBusy(true);
    setError(null);
    try {
      const next =
        mode === 'create'
          ? await commands.vaultCreate({
              name,
              password: password || null,
              rememberOnDevice: remember,
            })
          : await commands.vaultUnlock(name, password);
      setPassword('');
      setConfirm('');
      onChange(next);
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setBusy(false);
    }
  };

  const unlockWithDevice = async () => {
    setBusy(true);
    setError(null);
    try {
      onChange(await commands.vaultUnlock(name, null));
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="bg-bg safe-top safe-bottom flex min-h-full items-center justify-center overflow-y-auto px-4 py-10">
      <div className="w-full max-w-sm">
        <Brand />
        <form
          onSubmit={(e) => void submit(e)}
          className="bg-surface-1 flex flex-col gap-4 rounded-2xl p-6 shadow-[var(--shadow)]"
        >
          <div className="flex items-center gap-2">
            {mode === 'create' ? (
              <ShieldCheck className="text-accent size-5" />
            ) : (
              <Lock className="text-accent size-5" />
            )}
            <h2 className="text-[15px] font-semibold">
              {mode === 'create' ? t('lock.createTitle') : t('lock.unlockTitle')}
            </h2>
          </div>
          {mode === 'create' ? (
            <p className="text-muted text-[13px]">{t('lock.createDescription')}</p>
          ) : null}

          {mode === 'unlock' && status.vaults.length > 1 ? (
            <Field label={t('lock.vault')}>
              {(id) => (
                <Select id={id} value={name} onChange={(e) => setName(e.target.value)}>
                  {status.vaults.map((v) => (
                    <option key={v} value={v}>
                      {v}
                    </option>
                  ))}
                </Select>
              )}
            </Field>
          ) : null}
          {mode === 'create' ? (
            <Field label={t('lock.vaultName')} hint={t('lock.vaultNameHint')}>
              {(id) => <Input id={id} value={name} onChange={(e) => setName(e.target.value)} />}
            </Field>
          ) : null}

          <Field
            label={t('lock.masterPassword')}
            error={error}
            hint={mode === 'create' ? t('lock.passwordHint') : undefined}
          >
            {(id) => (
              <PasswordInput
                id={id}
                autoFocus
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                aria-invalid={Boolean(error)}
              />
            )}
          </Field>

          {mode === 'create' && password.length > 0 ? (
            <>
              <div className="flex gap-1" aria-label={t('lock.strength', { score: strength })}>
                {[0, 1, 2, 3].map((i) => (
                  <div
                    key={i}
                    className={`h-1 flex-1 rounded-full ${i < strength ? (strength < 2 ? 'bg-danger' : strength < 3 ? 'bg-warning' : 'bg-success') : 'bg-surface-4'}`}
                  />
                ))}
              </div>
              <Field
                label={t('lock.confirmPassword')}
                error={confirm && confirm !== password ? t('lock.mismatch') : undefined}
              >
                {(id) => (
                  <PasswordInput
                    id={id}
                    value={confirm}
                    onChange={(e) => setConfirm(e.target.value)}
                  />
                )}
              </Field>
            </>
          ) : null}

          {mode === 'create' ? (
            <div className="flex items-start justify-between gap-3">
              <span>
                <span className="block text-[13px]">{t('lock.rememberDevice')}</span>
                <span className="text-subtle block text-xs">
                  {t('lock.rememberDeviceHint', { store: platformStore(platform.kind) })}
                </span>
              </span>
              <Switch
                checked={remember}
                onCheckedChange={setRemember}
                label={t('lock.rememberDevice')}
              />
            </div>
          ) : null}

          <Button
            type="submit"
            variant="primary"
            size="lg"
            loading={busy}
            disabled={mode === 'create' ? !canCreate : password.length === 0}
          >
            <KeyRound className="size-4" />
            {mode === 'create' ? t('lock.create') : t('lock.unlock')}
          </Button>

          {mode === 'unlock' && status.hasDeviceKey ? (
            <Button variant="ghost" onClick={() => void unlockWithDevice()} disabled={busy}>
              <Fingerprint className="size-4" />
              {t('lock.unlockWithDevice', { store: platformStore(platform.kind) })}
            </Button>
          ) : null}
        </form>
        <div className="mt-4 text-center">
          <button
            type="button"
            className="text-muted hover:text-text text-[13px]"
            onClick={() => {
              setMode(mode === 'create' ? 'unlock' : 'create');
              setError(null);
              if (mode === 'unlock') setName('');
            }}
            hidden={creating}
          >
            {mode === 'create' ? t('lock.haveVault') : t('lock.newVault')}
          </button>
        </div>
        <p className="text-subtle mt-6 text-center text-xs">{t('lock.serverless')}</p>
      </div>
    </div>
  );
}

function platformStore(kind: string): string {
  switch (kind) {
    case 'macos':
    case 'ios':
      return 'Keychain';
    case 'windows':
      return 'Windows Credential Manager';
    case 'android':
      return 'Android Keystore';
    default:
      return 'Secret Service';
  }
}
