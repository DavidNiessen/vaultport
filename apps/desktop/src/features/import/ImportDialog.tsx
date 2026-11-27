// SPDX-License-Identifier: GPL-3.0-or-later
/** Import hosts, keys and known hosts from other clients and cloud providers. */
import { useQueryClient } from '@tanstack/react-query';
import { Cloud, FileText } from 'lucide-react';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';

import { Button } from '@/components/ui/button';
import { Select } from '@/components/ui/controls';
import { Field, Input, PasswordInput } from '@/components/ui/input';
import { Dialog } from '@/components/ui/overlay';
import { commands, errorMessage, type ImportFormat, type ImportSummary } from '@/lib/ipc';

const SOURCES: { id: ImportFormat; label: string; hint: string }[] = [
  { id: 'openSshConfig', label: 'OpenSSH (~/.ssh/config)', hint: 'import.openssh' },
  { id: 'knownHosts', label: 'known_hosts', hint: 'import.knownHosts' },
  { id: 'csv', label: 'CSV', hint: 'import.csv' },
  { id: 'puttyReg', label: 'PuTTY (.reg)', hint: 'import.putty' },
  { id: 'mobaXterm', label: 'MobaXterm (.mobaconf)', hint: 'import.moba' },
  { id: 'secureCrtXml', label: 'SecureCRT (.xml)', hint: 'import.securecrt' },
];

type Provider = 'aws' | 'digitalocean' | 'azure';

export function ImportDialog({
  open,
  onOpenChange,
}: {
  open: boolean;
  onOpenChange: (o: boolean) => void;
}) {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const [tab, setTab] = useState<'file' | 'cloud'>('file');
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<ImportSummary | null>(null);
  const [provider, setProvider] = useState<Provider>('aws');
  const [label, setLabel] = useState('');
  const [a, setA] = useState('');
  const [b, setB] = useState('');
  const [c, setC] = useState('');
  const [region, setRegion] = useState('us-east-1');
  const [subscriptions, setSubscriptions] = useState('');

  const refresh = async () => {
    for (const k of ['hosts', 'groups', 'keys', 'knownHosts', 'forwards', 'identities'])
      await qc.invalidateQueries({ queryKey: [k] });
  };

  const fromFile = async (format: ImportFormat | null) => {
    setBusy(true);
    try {
      let path: string | null = null;
      if (format === 'openSshConfig') path = await commands.defaultSshConfigPath();
      const dialog = await import('@tauri-apps/plugin-dialog');
      const chosen = path ?? (await dialog.open({ multiple: false }));
      if (typeof chosen !== 'string') return;
      const summary = await commands.importFile(chosen, format);
      setResult(summary);
      await refresh();
    } catch (e) {
      toast.error(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  const addCloud = async () => {
    setBusy(true);
    try {
      const credentials =
        provider === 'aws'
          ? { accessKeyId: a, secretAccessKey: b }
          : provider === 'digitalocean'
            ? { token: a }
            : { tenantId: a, clientId: b, clientSecret: c };
      const settings: Record<string, string> =
        provider === 'aws' ? { regions: region } : provider === 'azure' ? { subscriptions } : {};
      const n = await commands.cloudAddIntegration(
        provider,
        label || provider,
        JSON.stringify(credentials),
        settings,
      );
      toast.success(t('import.cloudSynced', { count: n }));
      await refresh();
      onOpenChange(false);
    } catch (e) {
      toast.error(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog
      open={open}
      onOpenChange={(o) => {
        setResult(null);
        onOpenChange(o);
      }}
      title={t('import.title')}
      description={t('import.description')}
      className="sm:w-[min(92vw,600px)]"
    >
      <div className="bg-surface-3 mb-4 inline-flex rounded-[var(--radius-control)] p-0.5">
        {(['file', 'cloud'] as const).map((x) => (
          <button
            key={x}
            type="button"
            onClick={() => setTab(x)}
            className={`flex h-8 items-center gap-1.5 rounded-[8px] px-3 text-[13px] font-medium ${tab === x ? 'bg-surface-1 shadow-sm' : 'text-muted'}`}
          >
            {x === 'file' ? <FileText className="size-4" /> : <Cloud className="size-4" />}
            {x === 'file' ? t('import.fromFile') : t('import.fromCloud')}
          </button>
        ))}
      </div>
      {tab === 'file' ? (
        result ? (
          <div className="grid gap-3">
            <p className="text-[13px]">
              {t('import.result', {
                hosts: result.hosts,
                groups: result.groups,
                keys: result.keys,
                knownHosts: result.knownHosts,
              })}
            </p>
            {result.warnings.length > 0 ? (
              <ul className="bg-warning-soft text-warning max-h-48 overflow-y-auto rounded-xl p-3 text-xs">
                {result.warnings.map((w, i) => (
                  <li key={String(i)}>{w}</li>
                ))}
              </ul>
            ) : null}
            <Button variant="primary" onClick={() => onOpenChange(false)}>
              {t('common.done')}
            </Button>
          </div>
        ) : (
          <div className="grid gap-2">
            {SOURCES.map((s) => (
              <button
                key={s.id}
                type="button"
                disabled={busy}
                onClick={() => void fromFile(s.id)}
                className="bg-surface-2 hover:bg-surface-3 flex flex-col items-start rounded-xl px-4 py-3 text-left"
              >
                <span className="text-[13px] font-medium">{s.label}</span>
                <span className="text-subtle text-xs">{t(s.hint)}</span>
              </button>
            ))}
            <Button variant="ghost" disabled={busy} onClick={() => void fromFile(null)}>
              {t('import.detect')}
            </Button>
          </div>
        )
      ) : (
        <div className="grid gap-3">
          <Field label={t('import.provider')}>
            {(id) => (
              <Select
                id={id}
                value={provider}
                onChange={(e) => setProvider(e.target.value as Provider)}
              >
                <option value="aws">AWS (EC2 / Lightsail)</option>
                <option value="digitalocean">DigitalOcean</option>
                <option value="azure">Microsoft Azure</option>
              </Select>
            )}
          </Field>
          <Field label={t('forwarding.label')}>
            {(id) => <Input id={id} value={label} onChange={(e) => setLabel(e.target.value)} />}
          </Field>
          {provider === 'aws' ? (
            <>
              <Field label="Access key ID">
                {(id) => <Input id={id} value={a} onChange={(e) => setA(e.target.value)} />}
              </Field>
              <Field label="Secret access key">
                {(id) => <PasswordInput id={id} value={b} onChange={(e) => setB(e.target.value)} />}
              </Field>
              <Field label={t('import.regions')} hint={t('import.regionsHint')}>
                {(id) => (
                  <Input id={id} value={region} onChange={(e) => setRegion(e.target.value)} />
                )}
              </Field>
            </>
          ) : provider === 'digitalocean' ? (
            <Field label={t('import.token')}>
              {(id) => <PasswordInput id={id} value={a} onChange={(e) => setA(e.target.value)} />}
            </Field>
          ) : (
            <>
              <Field label="Tenant ID">
                {(id) => <Input id={id} value={a} onChange={(e) => setA(e.target.value)} />}
              </Field>
              <Field label="Client ID">
                {(id) => <Input id={id} value={b} onChange={(e) => setB(e.target.value)} />}
              </Field>
              <Field label="Client secret">
                {(id) => <PasswordInput id={id} value={c} onChange={(e) => setC(e.target.value)} />}
              </Field>
              <Field label={t('import.subscriptions')} hint={t('import.subscriptionsHint')}>
                {(id) => (
                  <Input
                    id={id}
                    value={subscriptions}
                    onChange={(e) => setSubscriptions(e.target.value)}
                  />
                )}
              </Field>
            </>
          )}
          <p className="text-subtle text-xs">{t('import.cloudNote')}</p>
          <Button variant="primary" loading={busy} onClick={() => void addCloud()}>
            {t('import.addIntegration')}
          </Button>
        </div>
      )}
    </Dialog>
  );
}
