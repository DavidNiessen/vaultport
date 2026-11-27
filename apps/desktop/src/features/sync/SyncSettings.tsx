// SPDX-License-Identifier: GPL-3.0-or-later
import { useQuery, useQueryClient } from '@tanstack/react-query';
import {
  Cloud,
  Download,
  FolderSync,
  GitBranch,
  QrCode,
  RefreshCw,
  Server,
  Trash2,
  Upload,
} from 'lucide-react';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';

import { Button } from '@/components/ui/button';
import { Badge, Select } from '@/components/ui/controls';
import { Field, Input, PasswordInput } from '@/components/ui/input';
import { Card } from '@/components/ui/misc';
import { Dialog } from '@/components/ui/overlay';
import { commands, errorMessage, type SyncTargetConfig } from '@/lib/ipc';
import { EMPTY } from '@/lib/empty';

type Kind = SyncTargetConfig['type'];

function AddTargetDialog({ onClose }: { onClose: () => void }) {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const [kind, setKind] = useState<Kind>('folder');
  const [label, setLabel] = useState('');
  const [path, setPath] = useState('');
  const [url, setUrl] = useState('');
  const [user, setUser] = useState('');
  const [secret, setSecret] = useState('');
  const [bucket, setBucket] = useState('');
  const [region, setRegion] = useState('us-east-1');
  const [prefix, setPrefix] = useState('vaultport');
  const [busy, setBusy] = useState(false);

  const build = (): SyncTargetConfig => {
    switch (kind) {
      case 'folder':
        return { type: 'folder', path };
      case 'webdav':
        return { type: 'webdav', url, username: user };
      case 's3':
        return { type: 's3', endpoint: url || null, bucket, region, prefix, accessKeyId: user };
      case 'git':
        return { type: 'git', remote: url, branch: 'main' };
    }
  };

  const pickFolder = async () => {
    const { open } = await import('@tauri-apps/plugin-dialog');
    const p = await open({ directory: true });
    if (typeof p === 'string') setPath(p);
  };

  const submit = async () => {
    setBusy(true);
    try {
      await commands.syncAddTarget(label || kind, build(), secret || null);
      await qc.invalidateQueries({ queryKey: ['syncTargets'] });
      onClose();
    } catch (e) {
      toast.error(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog
      open
      onOpenChange={(o) => {
        if (!o) onClose();
      }}
      title={t('sync.addTarget')}
      description={t('sync.addTargetDescription')}
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            {t('common.cancel')}
          </Button>
          <Button variant="primary" loading={busy} onClick={() => void submit()}>
            {t('sync.connect')}
          </Button>
        </>
      }
    >
      <div className="grid gap-3">
        <Field label={t('sync.type')}>
          {(id) => (
            <Select id={id} value={kind} onChange={(e) => setKind(e.target.value as Kind)}>
              <option value="folder">{t('sync.folder')}</option>
              <option value="webdav">WebDAV (Nextcloud, ownCloud…)</option>
              <option value="s3">S3 (AWS, R2, MinIO, B2…)</option>
              <option value="git">{t('sync.git')}</option>
            </Select>
          )}
        </Field>
        <Field label={t('forwarding.label')}>
          {(id) => <Input id={id} value={label} onChange={(e) => setLabel(e.target.value)} />}
        </Field>
        {kind === 'folder' ? (
          <Field label={t('sync.folderPath')} hint={t('sync.folderHint')}>
            {(id) => (
              <div className="flex gap-2">
                <Input id={id} value={path} onChange={(e) => setPath(e.target.value)} />
                <Button onClick={() => void pickFolder()}>{t('sync.browse')}</Button>
              </div>
            )}
          </Field>
        ) : (
          <Field
            label={
              kind === 'git' ? t('sync.gitRemote') : kind === 's3' ? t('sync.endpoint') : 'URL'
            }
          >
            {(id) => (
              <Input
                id={id}
                value={url}
                placeholder={
                  kind === 'git'
                    ? 'git@github.com:me/vault.git'
                    : kind === 's3'
                      ? 'https://s3.amazonaws.com'
                      : 'https://cloud.example.com/remote.php/dav/files/me/'
                }
                onChange={(e) => setUrl(e.target.value)}
              />
            )}
          </Field>
        )}
        {kind === 's3' ? (
          <div className="grid grid-cols-3 gap-2">
            <Field label={t('sync.bucket')}>
              {(id) => <Input id={id} value={bucket} onChange={(e) => setBucket(e.target.value)} />}
            </Field>
            <Field label={t('sync.region')}>
              {(id) => <Input id={id} value={region} onChange={(e) => setRegion(e.target.value)} />}
            </Field>
            <Field label={t('sync.prefix')}>
              {(id) => <Input id={id} value={prefix} onChange={(e) => setPrefix(e.target.value)} />}
            </Field>
          </div>
        ) : null}
        {kind === 'webdav' || kind === 's3' ? (
          <div className="grid grid-cols-2 gap-2">
            <Field label={kind === 's3' ? t('sync.accessKey') : t('hosts.username')}>
              {(id) => <Input id={id} value={user} onChange={(e) => setUser(e.target.value)} />}
            </Field>
            <Field label={kind === 's3' ? t('sync.secretKey') : t('hosts.password')}>
              {(id) => (
                <PasswordInput id={id} value={secret} onChange={(e) => setSecret(e.target.value)} />
              )}
            </Field>
          </div>
        ) : null}
        <p className="text-subtle text-xs">{t('sync.e2eNote')}</p>
      </div>
    </Dialog>
  );
}

function PassphraseDialog({ mode, onClose }: { mode: 'export' | 'import'; onClose: () => void }) {
  const { t } = useTranslation();
  const [pw, setPw] = useState('');
  const [busy, setBusy] = useState(false);
  const run = async () => {
    setBusy(true);
    try {
      const dialog = await import('@tauri-apps/plugin-dialog');
      if (mode === 'export') {
        const path = await dialog.save({
          defaultPath: 'vault.vaultport',
          filters: [{ name: 'Vaultport vault', extensions: ['vaultport'] }],
        });
        if (path) {
          await commands.syncExportFile(path, pw);
          toast.success(t('sync.exported'));
        }
      } else {
        const path = await dialog.open({
          filters: [{ name: 'Vaultport vault', extensions: ['vaultport'] }],
        });
        if (typeof path === 'string') {
          const name = window.prompt(t('sync.importName'), 'restored') ?? '';
          if (!name) return;
          const n = await commands.syncImportFile(path, pw, name);
          toast.success(t('sync.importedOps', { count: n }));
        }
      }
      onClose();
    } catch (e) {
      toast.error(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <Dialog
      open
      onOpenChange={(o) => {
        if (!o) onClose();
      }}
      title={mode === 'export' ? t('sync.exportTitle') : t('sync.importTitle')}
      description={mode === 'export' ? t('sync.exportDescription') : t('sync.importDescription')}
      footer={
        <Button
          variant="primary"
          loading={busy}
          disabled={mode === 'export' ? pw.length < 8 : pw.length === 0}
          onClick={() => void run()}
        >
          {mode === 'export' ? t('sync.export') : t('sync.import')}
        </Button>
      }
    >
      <Field
        label={t('sync.filePassphrase')}
        hint={mode === 'export' ? t('sync.filePassphraseHint') : undefined}
      >
        {(id) => (
          <PasswordInput id={id} autoFocus value={pw} onChange={(e) => setPw(e.target.value)} />
        )}
      </Field>
    </Dialog>
  );
}

function PairingDialog({ onClose }: { onClose: () => void }) {
  const { t } = useTranslation();
  const pairing = useQuery({
    queryKey: ['pairing'],
    queryFn: () => commands.syncStartPairing(),
    staleTime: 0,
    gcTime: 0,
  });
  return (
    <Dialog
      open
      onOpenChange={(o) => {
        if (!o) onClose();
      }}
      title={t('sync.pairTitle')}
      description={t('sync.pairDescription')}
    >
      {pairing.data ? (
        <div className="flex flex-col items-center gap-4">
          <img
            alt={t('sync.pairQr')}
            className="size-56 rounded-xl bg-white p-3"
            src={`data:image/svg+xml;base64,${btoa(pairing.data.qrSvg)}`}
          />
          <div className="text-center">
            <p className="text-muted text-xs">{t('sync.pairCode')}</p>
            <p className="selectable font-mono text-2xl tracking-[0.3em]">{pairing.data.code}</p>
            <p className="text-subtle mt-1 text-xs">{t('sync.pairExpires')}</p>
            <button
              type="button"
              className="text-accent-text mt-2 text-xs"
              onClick={() => {
                void navigator.clipboard.writeText(pairing.data.payload);
                toast.success(t('sync.payloadCopied'));
              }}
            >
              {t('sync.copyPayload')}
            </button>
          </div>
        </div>
      ) : (
        <p className="text-danger text-[13px]">
          {pairing.error ? errorMessage(pairing.error) : t('common.loading')}
        </p>
      )}
    </Dialog>
  );
}

export function SyncSettings() {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const targets =
    useQuery({ queryKey: ['syncTargets'], queryFn: () => commands.syncListTargets() }).data ??
    EMPTY;
  const [dialog, setDialog] = useState<'add' | 'export' | 'import' | 'pair' | null>(null);
  const [syncing, setSyncing] = useState<string | null>(null);
  const icon = (k: Kind) =>
    k === 'folder' ? (
      <FolderSync />
    ) : k === 'git' ? (
      <GitBranch />
    ) : k === 's3' ? (
      <Cloud />
    ) : (
      <Server />
    );

  const syncNow = async (id: string) => {
    setSyncing(id);
    try {
      const r = await commands.syncNow(id);
      toast.success(t('sync.done', { up: r.uploaded, down: r.applied }));
    } catch (e) {
      toast.error(errorMessage(e));
    } finally {
      setSyncing(null);
      await qc.invalidateQueries({ queryKey: ['syncTargets'] });
    }
  };

  return (
    <>
      <Card
        title={t('sync.title')}
        actions={
          <Button size="sm" variant="primary" onClick={() => setDialog('add')}>
            {t('sync.addTarget')}
          </Button>
        }
      >
        <p className="text-muted mb-3 text-[13px]">{t('sync.description')}</p>
        {targets.length === 0 ? (
          <p className="text-subtle text-[13px]">{t('sync.noTargets')}</p>
        ) : null}
        <div className="grid gap-2">
          {targets.map((tg) => (
            <div key={tg.id} className="bg-surface-3 flex items-center gap-3 rounded-xl p-3">
              <span className="text-accent-text [&>svg]:size-5">{icon(tg.config.type)}</span>
              <div className="min-w-0 flex-1">
                <div className="flex items-center gap-2 text-[13px] font-medium">
                  {tg.label}
                  {tg.lastError ? (
                    <Badge tone="danger">{t('sync.error')}</Badge>
                  ) : tg.lastSyncAt ? (
                    <Badge tone="success">{t('sync.ok')}</Badge>
                  ) : null}
                </div>
                <div className="text-subtle truncate text-xs">
                  {tg.lastError ??
                    (tg.lastSyncAt
                      ? t('sync.lastSync', { time: new Date(tg.lastSyncAt).toLocaleString() })
                      : t('sync.never'))}
                </div>
              </div>
              <Button
                size="icon-sm"
                variant="ghost"
                aria-label={t('sync.syncNow')}
                onClick={() => void syncNow(tg.id)}
                loading={syncing === tg.id}
              >
                {syncing === tg.id ? null : <RefreshCw className="size-4" />}
              </Button>
              <Button
                size="icon-sm"
                variant="ghost"
                aria-label={t('common.delete')}
                onClick={() =>
                  void commands
                    .syncRemoveTarget(tg.id)
                    .then(() => qc.invalidateQueries({ queryKey: ['syncTargets'] }))
                }
              >
                <Trash2 className="size-4" />
              </Button>
            </div>
          ))}
        </div>
      </Card>
      <Card title={t('sync.devices')}>
        <p className="text-muted mb-3 text-[13px]">{t('sync.devicesDescription')}</p>
        <Button onClick={() => setDialog('pair')}>
          <QrCode className="size-4" />
          {t('sync.pairDevice')}
        </Button>
      </Card>
      <Card title={t('sync.backup')}>
        <p className="text-muted mb-3 text-[13px]">{t('sync.backupDescription')}</p>
        <div className="flex flex-wrap gap-2">
          <Button onClick={() => setDialog('export')}>
            <Download className="size-4" />
            {t('sync.export')}
          </Button>
          <Button onClick={() => setDialog('import')}>
            <Upload className="size-4" />
            {t('sync.import')}
          </Button>
        </div>
      </Card>
      {dialog === 'add' ? <AddTargetDialog onClose={() => setDialog(null)} /> : null}
      {dialog === 'export' || dialog === 'import' ? (
        <PassphraseDialog mode={dialog} onClose={() => setDialog(null)} />
      ) : null}
      {dialog === 'pair' ? <PairingDialog onClose={() => setDialog(null)} /> : null}
    </>
  );
}
