// SPDX-License-Identifier: GPL-3.0-or-later
import { writeText } from '@tauri-apps/plugin-clipboard-manager';
import {
  ChevronDown,
  Copy,
  Cpu,
  FileKey,
  KeyRound,
  Plus,
  ShieldCheck,
  Trash2,
  Upload,
  UserRound,
  Usb,
} from 'lucide-react';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';

import { Button } from '@/components/ui/button';
import { Badge, Segmented, Select, Switch } from '@/components/ui/controls';
import { Field, Input, PasswordInput, Textarea } from '@/components/ui/input';
import { EmptyState } from '@/components/ui/misc';
import { ContextMenu, Dialog, DropdownMenu } from '@/components/ui/overlay';
import { commands, errorMessage, newId, type Identity, type KeyMeta } from '@/lib/ipc';
import { EMPTY } from '@/lib/empty';
import {
  useDeleteIdentity,
  useHosts,
  useDeleteKey,
  useIdentities,
  useKeys,
  useSaveIdentity,
} from '@/lib/queries';

type KeyType = 'ed25519' | 'ecdsaP256' | 'ecdsaP384' | 'rsa';

function storageBadge(k: KeyMeta, t: (k: string) => string) {
  switch (k.storage.type) {
    case 'fido2':
      return <Badge tone="accent">{t('keychain.fido2')}</Badge>;
    case 'hardware':
      return <Badge tone="success">{t('keychain.hardware')}</Badge>;
    default:
      return k.encrypted ? <Badge>{t('keychain.passphrase')}</Badge> : null;
  }
}

function GenerateDialog({
  open,
  onOpenChange,
}: {
  open: boolean;
  onOpenChange: (o: boolean) => void;
}) {
  const { t } = useTranslation();
  const [label, setLabel] = useState('');
  const [type, setType] = useState<KeyType>('ed25519');
  const [passphrase, setPassphrase] = useState('');
  const [savePassphrase, setSavePassphrase] = useState(true);
  const [busy, setBusy] = useState(false);
  const submit = async () => {
    setBusy(true);
    try {
      await commands.keyGenerate({
        label: label || `${type} key`,
        algorithm: type,
        bits: type === 'rsa' ? 4096 : null,
        passphrase: passphrase || null,
        savePassphrase,
      });
      toast.success(t('keychain.generated'));
      setPassphrase('');
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
      onOpenChange={onOpenChange}
      title={t('keychain.generateTitle')}
      description={t('keychain.generateDescription')}
      footer={
        <>
          <Button variant="ghost" onClick={() => onOpenChange(false)}>
            {t('common.cancel')}
          </Button>
          <Button variant="primary" loading={busy} onClick={() => void submit()}>
            {t('keychain.generate')}
          </Button>
        </>
      }
    >
      <div className="grid gap-3">
        <Field label={t('keychain.label')}>
          {(id) => (
            <Input
              id={id}
              value={label}
              onChange={(e) => setLabel(e.target.value)}
              placeholder="Work laptop"
            />
          )}
        </Field>
        <Field label={t('keychain.keyType')} hint={t('keychain.keyTypeHint')}>
          {(id) => (
            <Select id={id} value={type} onChange={(e) => setType(e.target.value as KeyType)}>
              <option value="ed25519">Ed25519 ({t('keychain.recommended')})</option>
              <option value="ecdsaP256">ECDSA P-256</option>
              <option value="ecdsaP384">ECDSA P-384</option>
              <option value="rsa">RSA 4096</option>
            </Select>
          )}
        </Field>
        <Field label={t('keychain.passphraseOptional')}>
          {(id) => (
            <PasswordInput
              id={id}
              value={passphrase}
              onChange={(e) => setPassphrase(e.target.value)}
            />
          )}
        </Field>
        {passphrase ? (
          <label className="flex items-center justify-between gap-3 text-[13px]">
            {t('keychain.savePassphrase')}
            <Switch
              checked={savePassphrase}
              onCheckedChange={setSavePassphrase}
              label={t('keychain.savePassphrase')}
            />
          </label>
        ) : null}
      </div>
    </Dialog>
  );
}

function ImportDialog({
  open,
  onOpenChange,
}: {
  open: boolean;
  onOpenChange: (o: boolean) => void;
}) {
  const { t } = useTranslation();
  const [label, setLabel] = useState('');
  const [pem, setPem] = useState('');
  const [cert, setCert] = useState('');
  const [passphrase, setPassphrase] = useState('');
  const [busy, setBusy] = useState(false);
  const pickFile = async () => {
    const { open: openDialog } = await import('@tauri-apps/plugin-dialog');
    const path = await openDialog({ multiple: false, directory: false });
    if (typeof path === 'string') {
      try {
        setPem(await commands.readKeyFile(path));
        if (!label) setLabel(path.split(/[\\/]/).pop() ?? '');
      } catch (e) {
        toast.error(errorMessage(e));
      }
    }
  };
  const submit = async () => {
    setBusy(true);
    try {
      await commands.keyImport({
        label: label || 'Imported key',
        privateKey: pem,
        passphrase: passphrase || null,
        certificate: cert || null,
        savePassphrase: Boolean(passphrase),
      });
      setPem('');
      setPassphrase('');
      toast.success(t('keychain.imported'));
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
      onOpenChange={onOpenChange}
      title={t('keychain.importTitle')}
      description={t('keychain.importDescription')}
      footer={
        <>
          <Button variant="ghost" onClick={() => onOpenChange(false)}>
            {t('common.cancel')}
          </Button>
          <Button
            variant="primary"
            loading={busy}
            disabled={!pem.trim()}
            onClick={() => void submit()}
          >
            {t('keychain.import')}
          </Button>
        </>
      }
    >
      <div className="grid gap-3">
        <Field label={t('keychain.label')}>
          {(id) => <Input id={id} value={label} onChange={(e) => setLabel(e.target.value)} />}
        </Field>
        <Field label={t('keychain.privateKey')}>
          {(id) => (
            <div className="grid gap-2">
              <Textarea
                id={id}
                className="font-mono text-[11px]"
                rows={6}
                value={pem}
                onChange={(e) => setPem(e.target.value)}
                placeholder="-----BEGIN OPENSSH PRIVATE KEY-----"
              />
              <Button
                size="sm"
                variant="ghost"
                className="self-start"
                onClick={() => void pickFile()}
              >
                <FileKey className="size-4" />
                {t('keychain.chooseFile')}
              </Button>
            </div>
          )}
        </Field>
        <Field label={t('keychain.passphraseIfEncrypted')}>
          {(id) => (
            <PasswordInput
              id={id}
              value={passphrase}
              onChange={(e) => setPassphrase(e.target.value)}
            />
          )}
        </Field>
        <Field label={t('keychain.certificateOptional')}>
          {(id) => (
            <Textarea
              id={id}
              className="font-mono text-[11px]"
              rows={3}
              value={cert}
              onChange={(e) => setCert(e.target.value)}
              placeholder="ssh-ed25519-cert-v01@openssh.com AAAA…"
            />
          )}
        </Field>
      </div>
    </Dialog>
  );
}

function IdentityDialog({ identity, onClose }: { identity: Identity; onClose: () => void }) {
  const { t } = useTranslation();
  const keys = useKeys().data ?? EMPTY;
  const save = useSaveIdentity();
  const [draft, setDraft] = useState(identity);
  const [password, setPassword] = useState('');
  const submit = async () => {
    let next = draft;
    if (password) {
      const ref = await commands.storeSecret(password, draft.password ?? null, false);
      next = { ...draft, password: ref };
    }
    save.mutate(next, { onSuccess: onClose });
  };
  return (
    <Dialog
      open
      onOpenChange={(o) => {
        if (!o) onClose();
      }}
      title={identity.createdAt ? t('keychain.editIdentity') : t('keychain.newIdentity')}
      description={t('keychain.identityDescription')}
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            {t('common.cancel')}
          </Button>
          <Button
            variant="primary"
            disabled={!draft.label || !draft.username}
            onClick={() => void submit()}
          >
            {t('common.save')}
          </Button>
        </>
      }
    >
      <div className="grid gap-3">
        <Field label={t('keychain.label')}>
          {(id) => (
            <Input
              id={id}
              value={draft.label}
              onChange={(e) => setDraft({ ...draft, label: e.target.value })}
            />
          )}
        </Field>
        <Field label={t('hosts.username')}>
          {(id) => (
            <Input
              id={id}
              value={draft.username}
              onChange={(e) => setDraft({ ...draft, username: e.target.value })}
            />
          )}
        </Field>
        <Field
          label={t('hosts.password')}
          hint={draft.password ? t('hosts.passwordStored') : undefined}
        >
          {(id) => (
            <PasswordInput
              id={id}
              value={password}
              placeholder={draft.password ? '••••••••' : ''}
              onChange={(e) => setPassword(e.target.value)}
            />
          )}
        </Field>
        <Field label={t('hosts.keys')}>
          {() => (
            <div className="flex flex-col gap-1">
              {keys.length === 0 ? (
                <p className="text-subtle text-xs">{t('hosts.noKeys')}</p>
              ) : null}
              {keys.map((k) => (
                <label key={k.id} className="flex items-center gap-2 text-[13px]">
                  <input
                    type="checkbox"
                    className="accent-[var(--accent)]"
                    checked={draft.keys.includes(k.id)}
                    onChange={(e) =>
                      setDraft({
                        ...draft,
                        keys: e.target.checked
                          ? [...draft.keys, k.id]
                          : draft.keys.filter((x) => x !== k.id),
                      })
                    }
                  />
                  {k.label}
                </label>
              ))}
            </div>
          )}
        </Field>
      </div>
    </Dialog>
  );
}

function ExportDialog({ keyMeta, onClose }: { keyMeta: KeyMeta; onClose: () => void }) {
  const { t } = useTranslation();
  const hosts = useHosts().data ?? EMPTY;
  const [host, setHost] = useState('');
  const [busy, setBusy] = useState(false);
  const run = async () => {
    setBusy(true);
    try {
      const added = await commands.keyExportToHost(keyMeta.id, host);
      toast.success(added ? t('keychain.exported') : t('keychain.alreadyInstalled'));
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
      title={t('keychain.exportTitle', { label: keyMeta.label })}
      description={t('keychain.exportDescription')}
      footer={
        <Button variant="primary" loading={busy} disabled={!host} onClick={() => void run()}>
          {t('keychain.install')}
        </Button>
      }
    >
      <Field label={t('forwarding.host')}>
        {(id) => (
          <Select id={id} value={host} onChange={(e) => setHost(e.target.value)}>
            <option value="">{t('forwarding.chooseHost')}</option>
            {hosts.map((h) => (
              <option key={h.id} value={h.id}>
                {h.label}
              </option>
            ))}
          </Select>
        )}
      </Field>
    </Dialog>
  );
}

export function KeychainView() {
  const { t } = useTranslation();
  const keys = useKeys().data ?? EMPTY;
  const identities = useIdentities().data ?? EMPTY;
  const delKey = useDeleteKey();
  const delIdentity = useDeleteIdentity();
  const [tab, setTab] = useState<'keys' | 'identities'>('keys');
  const [dialog, setDialog] = useState<'generate' | 'import' | null>(null);
  const [editing, setEditing] = useState<Identity | null>(null);
  const [exporting, setExporting] = useState<KeyMeta | null>(null);

  const copyPublic = async (k: KeyMeta) => {
    await writeText(k.publicKey);
    toast.success(t('keychain.publicCopied'));
  };

  const newIdentity = (): Identity => ({
    id: newId(),
    label: '',
    username: '',
    password: null,
    keys: [],
    createdAt: 0,
    updatedAt: 0,
  });

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex flex-wrap items-center gap-2 px-4 pt-5 sm:px-6">
        <Segmented
          ariaLabel={t('nav.keychain')}
          value={tab}
          onChange={setTab}
          options={[
            { value: 'keys', label: t('keychain.keys'), icon: <KeyRound className="size-4" /> },
            {
              value: 'identities',
              label: t('keychain.identities'),
              icon: <UserRound className="size-4" />,
            },
          ]}
        />
        <div className="ml-auto flex">
          {tab === 'keys' ? (
            <DropdownMenu
              trigger={
                <Button variant="primary" size="sm">
                  <Plus className="size-4" />
                  <span className="font-semibold tracking-wide uppercase">
                    {t('keychain.newKey')}
                  </span>
                  <ChevronDown className="size-4" />
                </Button>
              }
              items={[
                {
                  label: t('keychain.generate'),
                  icon: <KeyRound />,
                  onSelect: () => setDialog('generate'),
                },
                {
                  label: t('keychain.import'),
                  icon: <Upload />,
                  onSelect: () => setDialog('import'),
                },
                { separator: true },
                {
                  label: t('keychain.newFido2'),
                  icon: <Usb />,
                  onSelect: () =>
                    void commands
                      .keyEnrollFido2('Security key')
                      .then(() => toast.success(t('keychain.generated')))
                      .catch((e: unknown) => toast.error(errorMessage(e))),
                },
                {
                  label: t('keychain.newHardware'),
                  icon: <Cpu />,
                  onSelect: () =>
                    void commands
                      .keyCreateHardware('Device key')
                      .then(() => toast.success(t('keychain.generated')))
                      .catch((e: unknown) => toast.error(errorMessage(e))),
                },
              ]}
            />
          ) : (
            <Button variant="primary" size="sm" onClick={() => setEditing(newIdentity())}>
              <Plus className="size-4" />
              <span className="font-semibold tracking-wide uppercase">
                {t('keychain.newIdentity')}
              </span>
            </Button>
          )}
        </div>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto px-4 py-4 sm:px-6">
        {tab === 'keys' ? (
          keys.length === 0 ? (
            <EmptyState
              icon={<KeyRound />}
              title={t('keychain.emptyKeys')}
              description={t('keychain.emptyKeysDescription')}
              action={
                <Button variant="primary" onClick={() => setDialog('generate')}>
                  {t('keychain.generate')}
                </Button>
              }
            />
          ) : (
            <div className="grid grid-cols-1 gap-2.5 lg:grid-cols-2 2xl:grid-cols-3">
              {keys.map((k) => (
                <ContextMenu
                  key={k.id}
                  items={[
                    {
                      label: t('keychain.copyPublic'),
                      icon: <Copy />,
                      onSelect: () => void copyPublic(k),
                    },
                    {
                      label: t('keychain.installOnHost'),
                      icon: <Upload />,
                      onSelect: () => setExporting(k),
                    },
                    { separator: true },
                    {
                      label: t('common.delete'),
                      icon: <Trash2 />,
                      danger: true,
                      onSelect: () => delKey.mutate(k.id),
                    },
                  ]}
                >
                  <div className="bg-surface-2 flex items-center gap-3 rounded-[var(--radius-card)] p-3.5">
                    <div className="bg-warning-soft text-warning flex size-9 shrink-0 items-center justify-center rounded-[11px]">
                      {k.storage.type === 'hardware' ? (
                        <ShieldCheck className="size-4" />
                      ) : (
                        <KeyRound className="size-4" />
                      )}
                    </div>
                    <div className="min-w-0 flex-1">
                      <div className="flex items-center gap-2">
                        <span className="truncate font-medium">{k.label}</span>
                        {storageBadge(k, t)}
                      </div>
                      <div className="text-subtle truncate font-mono text-[11px]">
                        {k.algorithm} · {k.fingerprint}
                      </div>
                      {k.certificate ? (
                        <Badge tone="accent" className="mt-1">
                          {t('keychain.certificate')}
                        </Badge>
                      ) : null}
                    </div>
                    <Button
                      size="icon-sm"
                      variant="ghost"
                      aria-label={t('keychain.copyPublic')}
                      onClick={() => void copyPublic(k)}
                    >
                      <Copy className="size-4" />
                    </Button>
                  </div>
                </ContextMenu>
              ))}
            </div>
          )
        ) : identities.length === 0 ? (
          <EmptyState
            icon={<UserRound />}
            title={t('keychain.emptyIdentities')}
            description={t('keychain.identityDescription')}
            action={
              <Button variant="primary" onClick={() => setEditing(newIdentity())}>
                {t('keychain.newIdentity')}
              </Button>
            }
          />
        ) : (
          <div className="grid grid-cols-1 gap-2.5 lg:grid-cols-2 2xl:grid-cols-3">
            {identities.map((i) => (
              <ContextMenu
                key={i.id}
                items={[
                  { label: t('common.edit'), onSelect: () => setEditing(i) },
                  {
                    label: t('common.delete'),
                    icon: <Trash2 />,
                    danger: true,
                    onSelect: () => delIdentity.mutate(i.id),
                  },
                ]}
              >
                <button
                  type="button"
                  onClick={() => setEditing(i)}
                  className="bg-surface-2 hover:bg-surface-3 flex items-center gap-3 rounded-[var(--radius-card)] p-3.5 text-left"
                >
                  <div className="bg-accent-soft text-accent-text flex size-9 items-center justify-center rounded-[11px]">
                    <UserRound className="size-4" />
                  </div>
                  <div className="min-w-0">
                    <div className="truncate font-medium">{i.label}</div>
                    <div className="text-subtle truncate text-xs">
                      {i.username}
                      {i.password ? ` · ${t('hosts.password')}` : ''}
                      {i.keys.length
                        ? ` · ${t('keychain.keyCount', { count: i.keys.length })}`
                        : ''}
                    </div>
                  </div>
                </button>
              </ContextMenu>
            ))}
          </div>
        )}
      </div>
      <GenerateDialog
        open={dialog === 'generate'}
        onOpenChange={(o) => setDialog(o ? 'generate' : null)}
      />
      <ImportDialog
        open={dialog === 'import'}
        onOpenChange={(o) => setDialog(o ? 'import' : null)}
      />
      {editing ? <IdentityDialog identity={editing} onClose={() => setEditing(null)} /> : null}
      {exporting ? <ExportDialog keyMeta={exporting} onClose={() => setExporting(null)} /> : null}
    </div>
  );
}
