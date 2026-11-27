// SPDX-License-Identifier: GPL-3.0-or-later
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { openUrl } from '@tauri-apps/plugin-opener';
import {
  Brain,
  Cloud,
  Info,
  Keyboard,
  Monitor,
  Palette,
  Shield,
  SquareTerminal,
} from 'lucide-react';
import { useState, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';

import { Button } from '@/components/ui/button';
import { Kbd, Segmented, Select, Switch } from '@/components/ui/controls';
import { Field, Input, PasswordInput } from '@/components/ui/input';
import { Card, Row } from '@/components/ui/misc';
import { cn } from '@/lib/cn';
import { commands, errorMessage, type VaultStatus } from '@/lib/ipc';
import { formatShortcut, useBreakpoint, usePlatform } from '@/lib/platform';
import { useSettings } from '@/lib/queries';
import { useApp, type ThemePref } from '@/stores/app';

import { AiSettingsCard } from '../ai/AiSettingsCard';
import { SHORTCUTS } from '../palette/shortcuts';
import { SyncSettings } from '../sync/SyncSettings';
import { DEFAULT_TERMINAL_SETTINGS, type TerminalSettings } from '../terminal/settings';
import { TERMINAL_FONTS, TERMINAL_THEMES, findTheme } from '../terminal/themes';
import { DEFAULT_SECURITY_SETTINGS, type SecuritySettings } from './security';

type Section = 'appearance' | 'terminal' | 'security' | 'sync' | 'ai' | 'shortcuts' | 'about';

const ACCENTS = [
  '#6366f1',
  '#0ea5e9',
  '#10b981',
  '#f59e0b',
  '#ef4444',
  '#ec4899',
  '#8b5cf6',
  '#64748b',
];

function ThemePreview({
  id,
  active,
  onClick,
}: {
  id: string;
  active: boolean;
  onClick: () => void;
}) {
  const theme = findTheme(id, 'dark');
  const c = theme.colors;
  return (
    <button
      type="button"
      onClick={onClick}
      aria-pressed={active}
      className={cn(
        'flex flex-col gap-1.5 rounded-xl p-1.5 text-left ring-2 ring-transparent',
        active && 'ring-accent',
      )}
    >
      <div
        className="flex h-14 flex-col justify-center gap-1 rounded-lg px-2.5"
        style={{ background: c.background }}
      >
        <div className="flex gap-1">
          <span className="h-1.5 w-6 rounded-full" style={{ background: c.green }} />
          <span className="h-1.5 w-10 rounded-full" style={{ background: c.foreground }} />
        </div>
        <div className="flex gap-1">
          <span className="h-1.5 w-4 rounded-full" style={{ background: c.blue }} />
          <span className="h-1.5 w-8 rounded-full" style={{ background: c.yellow }} />
          <span className="h-1.5 w-3 rounded-full" style={{ background: c.red }} />
        </div>
        <div className="flex gap-1">
          <span className="h-1.5 w-12 rounded-full" style={{ background: c.magenta }} />
        </div>
      </div>
      <span className="truncate px-0.5 text-[12px]">{theme.name}</span>
    </button>
  );
}

function Appearance() {
  const { t, i18n } = useTranslation();
  const { theme, setTheme, accent, setAccent } = useApp();
  return (
    <Card title={t('settings.appearance')}>
      <Row label={t('settings.appTheme')}>
        <Segmented<ThemePref>
          ariaLabel={t('settings.appTheme')}
          value={theme}
          onChange={setTheme}
          options={[
            { value: 'system', label: t('settings.system') },
            { value: 'dark', label: t('settings.dark') },
            { value: 'light', label: t('settings.light') },
          ]}
        />
      </Row>
      <Row label={t('settings.accent')}>
        <div className="flex gap-1.5">
          {ACCENTS.map((c) => (
            <button
              key={c}
              type="button"
              aria-label={c}
              aria-pressed={accent === c}
              onClick={() => setAccent(c)}
              className={cn(
                'size-6 rounded-full ring-2 ring-offset-2 ring-offset-[var(--surface-2)]',
                accent === c ? 'ring-text' : 'ring-transparent',
              )}
              style={{ background: c }}
            />
          ))}
        </div>
      </Row>
      <Row label={t('settings.language')}>
        <Select
          aria-label={t('settings.language')}
          value={i18n.resolvedLanguage}
          onChange={(e) => void i18n.changeLanguage(e.target.value)}
          className="w-40"
        >
          <option value="en">English</option>
          <option value="de">Deutsch</option>
        </Select>
      </Row>
    </Card>
  );
}

function Terminal() {
  const { t } = useTranslation();
  const { settings, update } = useSettings<TerminalSettings>('terminal', DEFAULT_TERMINAL_SETTINGS);
  const toggle = (key: keyof TerminalSettings, label: string, description?: string) => (
    <Row label={label} description={description}>
      <Switch
        label={label}
        checked={Boolean(settings[key])}
        onCheckedChange={(v) => update({ [key]: v })}
      />
    </Row>
  );
  return (
    <>
      <Card title={t('settings.terminalTheme')}>
        <div className="grid grid-cols-2 gap-1 sm:grid-cols-3 xl:grid-cols-4">
          {TERMINAL_THEMES.map((th) => (
            <ThemePreview
              key={th.id}
              id={th.id}
              active={settings.theme === th.id}
              onClick={() => update({ theme: th.id })}
            />
          ))}
        </div>
      </Card>
      <Card title={t('settings.font')}>
        <div className="grid grid-cols-[1fr_100px] gap-3">
          <Field label={t('settings.fontFamily')}>
            {(id) => (
              <Select
                id={id}
                value={settings.fontFamily}
                onChange={(e) => update({ fontFamily: e.target.value })}
              >
                {TERMINAL_FONTS.map((f) => (
                  <option key={f} value={f}>
                    {f}
                  </option>
                ))}
              </Select>
            )}
          </Field>
          <Field label={t('hosts.fontSize')}>
            {(id) => (
              <Input
                id={id}
                inputMode="numeric"
                value={settings.fontSize}
                onChange={(e) =>
                  update({ fontSize: Math.min(32, Math.max(8, Number(e.target.value) || 14)) })
                }
              />
            )}
          </Field>
        </div>
        <Row label={t('settings.cursor')}>
          <Segmented
            ariaLabel={t('settings.cursor')}
            value={settings.cursorStyle}
            onChange={(v) => update({ cursorStyle: v })}
            options={[
              { value: 'block', label: '█' },
              { value: 'bar', label: '▏' },
              { value: 'underline', label: '▁' },
            ]}
          />
        </Row>
      </Card>
      <Card title={t('settings.behaviour')}>
        {toggle('autocomplete', t('settings.autocomplete'), t('settings.autocompleteHint'))}
        {toggle('autoReconnect', t('settings.autoReconnect'))}
        {toggle('copyOnSelect', t('settings.copyOnSelect'))}
        {toggle('rightClickPaste', t('settings.rightClickPaste'))}
        {toggle('highlightKeywords', t('settings.highlightKeywords'))}
        {toggle('cursorBlink', t('settings.cursorBlink'))}
        {toggle('brightBold', t('settings.brightBold'))}
        {toggle('optionAsMeta', t('settings.optionAsMeta'))}
        {toggle('remoteTitles', t('settings.remoteTitles'))}
        {toggle(
          'shellIntegration',
          t('settings.shellIntegration'),
          t('settings.shellIntegrationHint'),
        )}
        {toggle('importHistory', t('settings.importHistory'), t('settings.execChannelsHint'))}
        {toggle('detectOs', t('settings.detectOs'), t('settings.execChannelsHint'))}
        <Row label={t('settings.osc52')} description={t('settings.osc52Hint')}>
          <Select
            aria-label={t('settings.osc52')}
            className="w-32"
            value={settings.osc52}
            onChange={(e) => update({ osc52: e.target.value as TerminalSettings['osc52'] })}
          >
            <option value="ask">{t('settings.ask')}</option>
            <option value="allow">{t('settings.allow')}</option>
            <option value="deny">{t('settings.deny')}</option>
          </Select>
        </Row>
        <Row label={t('settings.termType')}>
          <Select
            aria-label={t('settings.termType')}
            className="w-40"
            value={settings.termType}
            onChange={(e) => update({ termType: e.target.value })}
          >
            <option value="xterm-256color">xterm-256color</option>
            <option value="xterm">xterm</option>
            <option value="linux">linux</option>
            <option value="vt100">vt100</option>
          </Select>
        </Row>
        <Row label={t('settings.scrollback')}>
          <Input
            aria-label={t('settings.scrollback')}
            className="w-28"
            inputMode="numeric"
            value={settings.scrollback}
            onChange={(e) =>
              update({
                scrollback: Math.min(200_000, Math.max(100, Number(e.target.value) || 10_000)),
              })
            }
          />
        </Row>
      </Card>
    </>
  );
}

function Security({
  status,
  onStatus,
}: {
  status: VaultStatus;
  onStatus: (s: VaultStatus) => void;
}) {
  const { t } = useTranslation();
  const { settings, update } = useSettings<SecuritySettings>('security', DEFAULT_SECURITY_SETTINGS);
  const [pw, setPw] = useState('');
  const [pw2, setPw2] = useState('');
  const run = async (fn: () => Promise<VaultStatus>, ok: string) => {
    try {
      onStatus(await fn());
      toast.success(ok);
    } catch (e) {
      toast.error(errorMessage(e));
    }
  };
  return (
    <>
      <Card title={t('settings.vaultSecurity')}>
        <Row label={t('settings.autoLock')} description={t('settings.autoLockHint')}>
          <Select
            aria-label={t('settings.autoLock')}
            className="w-36"
            value={String(settings.autoLockMinutes)}
            onChange={(e) => update({ autoLockMinutes: Number(e.target.value) })}
          >
            {[1, 5, 15, 30, 60, 240, 0].map((m) => (
              <option key={m} value={m}>
                {m === 0 ? t('settings.never') : t('settings.minutes', { count: m })}
              </option>
            ))}
          </Select>
        </Row>
        <Row label={t('settings.lockOnSleep')}>
          <Switch
            label={t('settings.lockOnSleep')}
            checked={settings.lockOnSleep}
            onCheckedChange={(v) => update({ lockOnSleep: v })}
          />
        </Row>
        <Row
          label={t('settings.lockOnBackground')}
          description={t('settings.lockOnBackgroundHint')}
        >
          <Switch
            label={t('settings.lockOnBackground')}
            checked={settings.lockOnBackground}
            onCheckedChange={(v) => update({ lockOnBackground: v })}
          />
        </Row>
        <Row label={t('settings.deviceUnlock')} description={t('settings.deviceUnlockHint')}>
          <Switch
            label={t('settings.deviceUnlock')}
            checked={status.hasDeviceKey}
            onCheckedChange={(v) =>
              void run(() => commands.vaultSetDeviceUnlock(v), t('settings.saved'))
            }
          />
        </Row>
        <Row label={t('settings.clearClipboard')} description={t('settings.clearClipboardHint')}>
          <Switch
            label={t('settings.clearClipboard')}
            checked={settings.clearClipboard}
            onCheckedChange={(v) => update({ clearClipboard: v })}
          />
        </Row>
      </Card>
      <Card title={status.hasPassword ? t('settings.changePassword') : t('settings.setPassword')}>
        <div className="grid gap-3 sm:grid-cols-2">
          <Field label={t('lock.masterPassword')}>
            {(id) => <PasswordInput id={id} value={pw} onChange={(e) => setPw(e.target.value)} />}
          </Field>
          <Field
            label={t('lock.confirmPassword')}
            error={pw2 && pw !== pw2 ? t('lock.mismatch') : undefined}
          >
            {(id) => <PasswordInput id={id} value={pw2} onChange={(e) => setPw2(e.target.value)} />}
          </Field>
        </div>
        <div className="mt-3 flex gap-2">
          <Button
            variant="primary"
            disabled={pw.length < 8 || pw !== pw2}
            onClick={() =>
              void run(() => commands.vaultSetPassword(pw), t('settings.passwordChanged')).then(
                () => {
                  setPw('');
                  setPw2('');
                },
              )
            }
          >
            {t('common.save')}
          </Button>
          {status.hasPassword && status.hasDeviceKey ? (
            <Button
              variant="ghost"
              className="text-danger"
              onClick={() => void run(() => commands.vaultRemovePassword(), t('settings.saved'))}
            >
              {t('settings.removePassword')}
            </Button>
          ) : null}
        </div>
      </Card>
    </>
  );
}

function Shortcuts() {
  const { t } = useTranslation();
  const platform = usePlatform();
  return (
    <Card title={t('settings.shortcuts')}>
      {SHORTCUTS.map((s) => (
        <Row key={s.id} label={t(s.label)}>
          <span className="flex gap-1">
            {formatShortcut(platform, s.keys)
              .split(/(?<=.)\+|(?=[⌘⌥⇧⌃])/)
              .filter(Boolean)
              .map((k, i) => (
                <Kbd key={`${k}-${String(i)}`}>{k}</Kbd>
              ))}
          </span>
        </Row>
      ))}
    </Card>
  );
}

function About() {
  const { t } = useTranslation();
  const platform = usePlatform();
  const qc = useQueryClient();
  const update = useQuery({
    queryKey: ['update'],
    queryFn: () => commands.updateCheck(),
    enabled: false,
  });
  return (
    <Card title={t('settings.about')}>
      <div className="flex items-center gap-4">
        <img src="/logo.svg" alt="" className="size-14" />
        <div>
          <div className="text-[15px] font-semibold">ssh-client</div>
          <div className="text-muted text-[13px]">
            {t('settings.version', { version: platform.version })} · {platform.kind} {platform.arch}
          </div>
          <div className="text-subtle text-xs">GPL-3.0-or-later</div>
        </div>
      </div>
      <p className="text-muted mt-4 text-[13px]">{t('settings.aboutText')}</p>
      <div className="mt-4 flex flex-wrap gap-2">
        {!platform.mobile ? (
          <Button
            loading={update.isFetching}
            onClick={() =>
              void qc
                .query({ queryKey: ['update'], queryFn: () => commands.updateCheck() })
                .then((u) => {
                  if (u)
                    toast(t('settings.updateAvailable', { version: u.version }), {
                      action: {
                        label: t('settings.install'),
                        onClick: () => void commands.updateInstall(),
                      },
                    });
                  else toast.success(t('settings.upToDate'));
                })
                .catch((e: unknown) => toast.error(errorMessage(e)))
            }
          >
            {t('settings.checkUpdates')}
          </Button>
        ) : null}
        <Button
          variant="ghost"
          onClick={() => void openUrl('https://github.com/davidniessen/ssh-client')}
        >
          {t('settings.sourceCode')}
        </Button>
        <Button
          variant="ghost"
          onClick={() => void openUrl('https://github.com/davidniessen/ssh-client/security/policy')}
        >
          {t('settings.securityPolicy')}
        </Button>
      </div>
    </Card>
  );
}

export function SettingsView({
  status,
  onStatus,
}: {
  status: VaultStatus;
  onStatus: (s: VaultStatus) => void;
}) {
  const { t } = useTranslation();
  const bp = useBreakpoint();
  const [section, setSection] = useState<Section>('appearance');
  const items: { id: Section; icon: ReactNode; label: string }[] = [
    { id: 'appearance', icon: <Palette />, label: t('settings.appearance') },
    { id: 'terminal', icon: <SquareTerminal />, label: t('settings.terminal') },
    { id: 'security', icon: <Shield />, label: t('settings.security') },
    { id: 'sync', icon: <Cloud />, label: t('settings.sync') },
    { id: 'ai', icon: <Brain />, label: t('settings.ai') },
    { id: 'shortcuts', icon: <Keyboard />, label: t('settings.shortcuts') },
    { id: 'about', icon: <Info />, label: t('settings.about') },
  ];
  return (
    <div className={cn('flex h-full min-h-0', bp === 'phone' ? 'flex-col' : 'flex-row')}>
      <nav
        aria-label={t('nav.settings')}
        className={cn(
          bp === 'phone'
            ? 'flex gap-1 overflow-x-auto px-4 pt-4'
            : 'flex w-52 shrink-0 flex-col gap-1 px-3 pt-5',
        )}
      >
        {items.map((i) => (
          <button
            key={i.id}
            type="button"
            aria-current={section === i.id ? 'page' : undefined}
            onClick={() => setSection(i.id)}
            className={cn(
              'flex h-9 shrink-0 items-center gap-2.5 rounded-[10px] px-3 text-[13px] font-medium [&>svg]:size-4',
              section === i.id
                ? 'bg-surface-3 text-text'
                : 'text-muted hover:bg-surface-2 hover:text-text',
            )}
          >
            {i.icon}
            {i.label}
          </button>
        ))}
      </nav>
      <div className="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto px-4 py-5 sm:px-6">
        <div className="mx-auto flex w-full max-w-3xl flex-col gap-3">
          {section === 'appearance' && <Appearance />}
          {section === 'terminal' && <Terminal />}
          {section === 'security' && <Security status={status} onStatus={onStatus} />}
          {section === 'sync' && <SyncSettings />}
          {section === 'ai' && <AiSettingsCard />}
          {section === 'shortcuts' && <Shortcuts />}
          {section === 'about' && <About />}
          <p className="text-subtle flex items-center gap-1.5 pt-2 text-xs">
            <Monitor className="size-3.5" />
            {t('settings.storedEncrypted')}
          </p>
        </div>
      </div>
    </div>
  );
}
