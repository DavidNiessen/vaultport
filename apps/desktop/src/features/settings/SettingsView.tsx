// SPDX-License-Identifier: GPL-3.0-or-later
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { openUrl } from '@tauri-apps/plugin-opener';
import {
  Bot,
  Brain,
  Check,
  Cloud,
  Info,
  Keyboard,
  Monitor,
  Palette,
  Plus,
  RotateCcw,
  Shield,
  SquareTerminal,
  X,
} from 'lucide-react';
import { useState, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';

import { Logo } from '@/components/Logo';
import { Button } from '@/components/ui/button';
import { Kbd, Segmented, Select, Switch } from '@/components/ui/controls';
import { Field, Input, PasswordInput } from '@/components/ui/input';
import { Card, PageHeader, Row } from '@/components/ui/misc';
import { cn } from '@/lib/cn';
import { commands, errorMessage, type VaultStatus } from '@/lib/ipc';
import { formatShortcut, useBreakpoint, usePlatform } from '@/lib/platform';
import { PRESETS, type PresetId } from '@/lib/presets';
import { useSettings } from '@/lib/queries';
import { useApp, type ThemePref } from '@/stores/app';

import { AgentSettingsCard } from '../agents/AgentSettings';
import { AiSettingsCard } from '../ai/AiSettingsCard';
import {
  SHORTCUTS,
  SHORTCUTS_DOC,
  SHORTCUT_GROUPS,
  bindingFromEvent,
  conflicts,
  useKeymap,
  type ShortcutId,
  type ShortcutOverrides,
} from '../palette/shortcuts';
import { SyncSettings } from '../sync/SyncSettings';
import { isValidPattern } from '../terminal/highlight';
import {
  DEFAULT_TERMINAL_SETTINGS,
  type HighlightRule,
  type TerminalSettings,
} from '../terminal/settings';
import {
  TERMINAL_FONTS,
  TERMINAL_THEMES,
  findTheme,
  themePreviewBackground,
} from '../terminal/themes';
import { DEFAULT_LOGGING_SETTINGS, type LoggingSettings } from './logging';
import { DEFAULT_SECURITY_SETTINGS, type SecuritySettings } from './security';

type Section =
  'appearance' | 'terminal' | 'security' | 'sync' | 'ai' | 'agents' | 'shortcuts' | 'about';

/** Colour preset tiles: a tiny window with the preset's glow, an accent pill and its name. */
function PresetPicker({ value, onChange }: { value: PresetId; onChange: (p: PresetId) => void }) {
  const { t } = useTranslation();
  return (
    <div
      role="radiogroup"
      aria-label={t('settings.preset')}
      className="mt-3 grid grid-cols-2 gap-2.5 sm:grid-cols-3 xl:grid-cols-6"
    >
      {PRESETS.map((p) => {
        const active = value === p.id;
        return (
          <button
            key={p.id}
            type="button"
            role="radio"
            aria-checked={active}
            onClick={() => onChange(p.id)}
            className={cn(
              'group flex flex-col gap-2 rounded-[14px] border p-1.5 text-left transition-colors duration-150',
              active
                ? 'border-accent bg-accent-soft'
                : 'border-border hover:border-border-strong hover:bg-surface-3',
            )}
          >
            <span
              className="relative flex h-16 items-end overflow-hidden rounded-[10px] p-2"
              style={{
                background: `radial-gradient(80% 90% at 15% 0%, ${p.glow[0]}55, transparent 70%), radial-gradient(70% 80% at 100% 100%, ${p.glow[1]}44, transparent 70%), #0d1115`,
              }}
              aria-hidden
            >
              <span className="h-2 w-10 rounded-full" style={{ background: p.swatch }} />
              <span className="ml-1 h-2 w-5 rounded-full bg-white/25" />
            </span>
            <span className="flex items-center justify-between px-1 pb-0.5 text-[13px] font-bold">
              {t(`settings.presets.${p.id}`)}
              {active ? <Check className="text-accent-text size-3.5" aria-hidden /> : null}
            </span>
          </button>
        );
      })}
    </div>
  );
}

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
        style={{ background: themePreviewBackground(theme) }}
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
  const { theme, setTheme, preset, setPreset, animatedBackground, setAnimatedBackground } =
    useApp();
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
      <div className="py-2">
        <div className="text-[13px] font-semibold">{t('settings.preset')}</div>
        <div className="text-muted mt-0.5 text-xs">{t('settings.presetHint')}</div>
        <PresetPicker value={preset} onChange={setPreset} />
      </div>
      <Row
        label={t('settings.animatedBackground')}
        description={t('settings.animatedBackgroundHint')}
      >
        <Switch
          checked={animatedBackground}
          onCheckedChange={setAnimatedBackground}
          label={t('settings.animatedBackground')}
        />
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
  const platform = usePlatform();
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
      <Card title={t('settings.suggestions')}>
        {toggle('autocomplete', t('settings.autocomplete'), t('settings.autocompleteHint'))}
        <Row label={t('settings.suggestMode')} description={t('settings.suggestModeHint')}>
          <Segmented
            ariaLabel={t('settings.suggestMode')}
            value={settings.suggestMode}
            onChange={(v) => update({ suggestMode: v })}
            options={[
              { value: 'auto', label: t('settings.suggestAuto') },
              { value: 'tab', label: t('settings.suggestTab') },
            ]}
          />
        </Row>
        {toggle('ghostText', t('settings.ghostText'), t('settings.ghostTextHint'))}
        {toggle(
          'syntaxHighlight',
          t('settings.syntaxHighlight'),
          t('settings.syntaxHighlightHint'),
        )}
        {toggle('aiHashPrompt', t('settings.aiHashPrompt'), t('settings.aiHashPromptHint'))}
      </Card>
      <Card title={t('settings.commandBlocks')}>
        {toggle(
          'shellIntegration',
          t('settings.shellIntegration'),
          t('settings.shellIntegrationHint'),
        )}
        <Row label={t('settings.sshIntegration')} description={t('settings.sshIntegrationHint')}>
          <Select
            aria-label={t('settings.sshIntegration')}
            className="w-52"
            value={settings.sshIntegration}
            onChange={(e) =>
              update({ sshIntegration: e.target.value as TerminalSettings['sshIntegration'] })
            }
          >
            <option value="off">{t('settings.sshIntegrationOff')}</option>
            <option value="auto">{t('settings.sshIntegrationAuto')}</option>
          </Select>
        </Row>
        {toggle('commandBlocks', t('settings.commandBlocks'), t('settings.commandBlocksHint'))}
        <Row label={t('settings.notifyAfter')} description={t('settings.notifyAfterHint')}>
          <span className="flex items-center gap-2">
            <Input
              aria-label={t('settings.notifyAfter')}
              className="w-20"
              inputMode="numeric"
              value={settings.notifyAfterSeconds}
              onChange={(e) =>
                update({
                  notifyAfterSeconds: Math.min(86_400, Math.max(0, Number(e.target.value) || 0)),
                })
              }
            />
            <span className="text-subtle text-[13px]">{t('settings.seconds')}</span>
          </span>
        </Row>
      </Card>
      <Card title={t('settings.highlightRules')}>
        {toggle('highlightKeywords', t('settings.highlightKeywords'))}
        <HighlightRules
          rules={settings.highlightRules}
          onChange={(highlightRules) => update({ highlightRules })}
        />
      </Card>
      <Card title={t('settings.behaviour')}>
        {toggle('autoReconnect', t('settings.autoReconnect'))}
        {toggle('copyOnSelect', t('settings.copyOnSelect'))}
        {toggle('rightClickPaste', t('settings.rightClickPaste'))}
        {toggle('cursorBlink', t('settings.cursorBlink'))}
        {toggle('brightBold', t('settings.brightBold'))}
        {toggle('optionAsMeta', t('settings.optionAsMeta'))}
        {toggle('remoteTitles', t('settings.remoteTitles'))}
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
        {!platform.mobile ? (
          <Row label={t('settings.localShell')} description={t('settings.localShellHint')}>
            <Input
              key={settings.localShell}
              aria-label={t('settings.localShell')}
              className="w-48 font-mono"
              spellCheck={false}
              autoCapitalize="off"
              autoCorrect="off"
              placeholder={t('settings.systemDefault')}
              defaultValue={settings.localShell}
              onBlur={(e) => {
                const value = e.target.value.trim();
                if (value !== settings.localShell) update({ localShell: value });
              }}
            />
          </Row>
        ) : null}
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

/** Editor for the user's own output highlighting rules. */
function HighlightRules({
  rules,
  onChange,
}: {
  rules: HighlightRule[];
  onChange: (rules: HighlightRule[]) => void;
}) {
  const { t } = useTranslation();
  const set = (i: number, patch: Partial<HighlightRule>) =>
    onChange(rules.map((r, j) => (j === i ? { ...r, ...patch } : r)));
  return (
    <div className="grid gap-2 py-2">
      <p className="text-subtle text-[12px]">{t('settings.highlightRulesHint')}</p>
      {rules.map((r, i) => {
        const valid = isValidPattern(r.pattern);
        return (
          <div key={i} className="flex flex-wrap items-center gap-2">
            <Input
              aria-label={t('settings.rulePattern')}
              placeholder={t('settings.rulePattern')}
              className={cn('min-w-0 flex-1 font-mono', !valid && r.pattern && 'border-danger')}
              spellCheck={false}
              defaultValue={r.pattern}
              title={!valid && r.pattern ? t('settings.invalidPattern') : undefined}
              onBlur={(e) => {
                if (e.target.value !== r.pattern) set(i, { pattern: e.target.value });
              }}
            />
            <input
              type="color"
              aria-label={t('settings.ruleColor')}
              value={r.color}
              onChange={(e) => set(i, { color: e.target.value })}
              className="border-border h-9 w-10 cursor-pointer rounded-lg border bg-transparent p-1"
            />
            <Switch
              label={t('settings.ruleCase')}
              checked={r.caseSensitive}
              onCheckedChange={(v) => set(i, { caseSensitive: v })}
            />
            <Button
              size="icon-sm"
              variant="ghost"
              aria-label={t('settings.removeRule')}
              onClick={() => onChange(rules.filter((_, j) => j !== i))}
            >
              <X className="size-4" />
            </Button>
          </div>
        );
      })}
      <Button
        variant="ghost"
        className="justify-self-start"
        onClick={() =>
          onChange([...rules, { pattern: '', color: '#f4c15c', caseSensitive: false }])
        }
      >
        <Plus className="size-4" />
        {t('settings.addRule')}
      </Button>
    </div>
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
  const platform = usePlatform();
  const { settings, update } = useSettings<SecuritySettings>('security', DEFAULT_SECURITY_SETTINGS);
  const logging = useSettings<LoggingSettings>('logging', DEFAULT_LOGGING_SETTINGS);
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
        {!platform.mobile ? (
          <Row label={t('settings.lockOnSleep')} description={t('settings.lockOnSleepHint')}>
            <Switch
              label={t('settings.lockOnSleep')}
              checked={settings.lockOnSleep}
              onCheckedChange={(v) => update({ lockOnSleep: v })}
            />
          </Row>
        ) : null}
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
      </Card>
      <Card title={t('settings.sessionLogs')}>
        <Row label={t('settings.recordSessions')} description={t('settings.recordSessionsHint')}>
          <Switch
            label={t('settings.recordSessions')}
            checked={logging.settings.enabled}
            onCheckedChange={(v) => logging.update({ enabled: v })}
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

function KeyCaps({ binding }: { binding: string }) {
  const platform = usePlatform();
  return (
    <span className="flex gap-1">
      {formatShortcut(platform, binding)
        .split(/(?<=.)\+|(?=[⌘⌥⇧⌃])/)
        .filter(Boolean)
        .map((k, i) => (
          <Kbd key={`${k}-${String(i)}`}>{k}</Kbd>
        ))}
    </span>
  );
}

/** Records the next key combination pressed while it has focus. */
function ShortcutRecorder({
  onRecord,
  onCancel,
}: {
  onRecord: (binding: string) => void;
  onCancel: () => void;
}) {
  const { t } = useTranslation();
  const apple = useKeymap((s) => s.apple);
  const setRecording = useKeymap((s) => s.setRecording);
  return (
    <input
      autoFocus
      readOnly
      aria-label={t('settings.shortcutPress')}
      placeholder={t('settings.shortcutPress')}
      onFocus={() => setRecording(true)}
      onBlur={() => {
        setRecording(false);
        onCancel();
      }}
      onKeyDown={(e) => {
        e.preventDefault();
        e.stopPropagation();
        if (e.key === 'Escape' && !e.ctrlKey && !e.metaKey && !e.altKey && !e.shiftKey) {
          onCancel();
          return;
        }
        const binding = bindingFromEvent(e.nativeEvent, apple);
        if (binding) {
          setRecording(false);
          onRecord(binding);
        }
      }}
      className="bg-surface-3 border-accent text-text h-7 w-40 rounded-md border px-2 text-[12px] outline-none"
    />
  );
}

function Shortcuts() {
  const { t } = useTranslation();
  const { settings: overrides, update } = useSettings<ShortcutOverrides>(SHORTCUTS_DOC, {});
  const keymap = useKeymap((s) => s.keymap);
  const apple = useKeymap((s) => s.apple);
  const [recording, setRecording] = useState<ShortcutId | null>(null);
  const clashes = conflicts(keymap);
  const set = (id: ShortcutId, keys: string[]) => update({ ...overrides, [id]: keys });
  const reset = (id: ShortcutId) => {
    update(Object.fromEntries(Object.entries(overrides).filter(([k]) => k !== id)));
  };
  return (
    <>
      <p className="text-muted mb-4 text-[13px]">{t('settings.shortcutsHint')}</p>
      {SHORTCUT_GROUPS.map((group) => (
        <Card key={group} title={t(`settings.shortcutGroup.${group}`)}>
          {SHORTCUTS.filter((s) => s.group === group).map((s) => {
            const keys = keymap[s.id];
            const custom = overrides[s.id] !== undefined;
            const defaults = apple ? s.apple : s.other;
            return (
              <Row key={s.id} label={t(s.label)}>
                <span className="flex flex-wrap items-center justify-end gap-1.5">
                  {keys.length === 0 && recording !== s.id ? (
                    <span className="text-subtle text-[12px]">{t('settings.shortcutNone')}</span>
                  ) : null}
                  {keys.map((k) => (
                    <span
                      key={k}
                      className={cn(
                        'group/key flex items-center gap-0.5 rounded-md',
                        clashes.has(k.toLowerCase()) && 'ring-warning ring-1',
                      )}
                      title={
                        clashes.has(k.toLowerCase())
                          ? t('settings.shortcutConflict', {
                              actions: (clashes.get(k.toLowerCase()) ?? [])
                                .filter((id) => id !== s.id)
                                .map((id) => t(`shortcuts.${id}`))
                                .join(', '),
                            })
                          : undefined
                      }
                    >
                      <KeyCaps binding={k} />
                      <button
                        type="button"
                        aria-label={t('settings.shortcutRemove', { keys: k })}
                        onClick={() =>
                          set(
                            s.id,
                            keys.filter((x) => x !== k),
                          )
                        }
                        className="text-subtle hover:text-text flex size-4 items-center justify-center rounded opacity-0 group-hover/key:opacity-100 focus:opacity-100"
                      >
                        <X className="size-3" />
                      </button>
                    </span>
                  ))}
                  {recording === s.id ? (
                    <ShortcutRecorder
                      onCancel={() => setRecording(null)}
                      onRecord={(k) => {
                        setRecording(null);
                        if (!keys.includes(k)) set(s.id, [...keys, k]);
                      }}
                    />
                  ) : (
                    <Button
                      size="icon-sm"
                      variant="ghost"
                      aria-label={t('settings.shortcutAdd', { action: t(s.label) })}
                      title={t('settings.shortcutAdd', { action: t(s.label) })}
                      onClick={() => setRecording(s.id)}
                    >
                      <Plus className="size-3.5" />
                    </Button>
                  )}
                  {custom ? (
                    <Button
                      size="icon-sm"
                      variant="ghost"
                      aria-label={t('settings.shortcutReset')}
                      title={`${t('settings.shortcutReset')}: ${defaults.join(', ') || t('settings.shortcutNone')}`}
                      onClick={() => reset(s.id)}
                    >
                      <RotateCcw className="size-3.5" />
                    </Button>
                  ) : null}
                </span>
              </Row>
            );
          })}
        </Card>
      ))}
      <Card title={t('settings.shortcutGroup.fixed')}>
        <Row label={t('settings.shortcutGoTo')}>
          <KeyCaps binding="Mod+1" />
        </Row>
        {apple ? (
          <>
            <Row label={t('settings.shortcutLineStartEnd')}>
              <span className="flex gap-2">
                <KeyCaps binding="Mod+←" />
                <KeyCaps binding="Mod+→" />
              </span>
            </Row>
            <Row label={t('settings.shortcutWordJump')}>
              <span className="flex gap-2">
                <KeyCaps binding="Alt+←" />
                <KeyCaps binding="Alt+→" />
              </span>
            </Row>
            <Row label={t('settings.shortcutDeleteLine')}>
              <KeyCaps binding="Mod+⌫" />
            </Row>
          </>
        ) : null}
        <Row label={t('settings.shortcutAccept')}>
          <span className="flex gap-2">
            <KeyCaps binding="Tab" />
            <KeyCaps binding="→" />
          </span>
        </Row>
      </Card>
      {Object.keys(overrides).length > 0 ? (
        <Button variant="ghost" onClick={() => update({})}>
          <RotateCcw className="size-4" />
          {t('settings.shortcutResetAll')}
        </Button>
      ) : null}
    </>
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
        <Logo className="size-14" />
        <div>
          <div className="text-[15px] font-semibold">Vaultport</div>
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
          onClick={() => void openUrl('https://github.com/DavidNiessen/vaultport')}
        >
          {t('settings.sourceCode')}
        </Button>
        <Button
          variant="ghost"
          onClick={() => void openUrl('https://github.com/DavidNiessen/vaultport/security/policy')}
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
    { id: 'agents', icon: <Bot />, label: t('settings.agents') },
    { id: 'shortcuts', icon: <Keyboard />, label: t('settings.shortcuts') },
    { id: 'about', icon: <Info />, label: t('settings.about') },
  ];
  return (
    <div className="flex h-full min-h-0 flex-col">
      {bp === 'phone' ? null : (
        <PageHeader
          title={t('nav.settings')}
          description={t('settings.intro')}
          className="px-4 pt-4 sm:px-8 sm:pt-7"
        />
      )}
      <div className={cn('flex min-h-0 flex-1', bp === 'phone' ? 'flex-col' : 'flex-row')}>
        <nav
          aria-label={t('nav.settings')}
          className={cn(
            bp === 'phone'
              ? 'flex gap-1 overflow-x-auto px-4 pt-4'
              : 'flex w-56 shrink-0 flex-col gap-1 pt-6 pr-2 pl-4 sm:pl-6',
          )}
        >
          {items.map((i) => (
            <button
              key={i.id}
              type="button"
              aria-current={section === i.id ? 'page' : undefined}
              onClick={() => setSection(i.id)}
              className={cn(
                'flex h-9 shrink-0 items-center gap-2.5 rounded-full px-3.5 text-[13.5px] font-bold transition-colors duration-150 [&>svg]:size-4',
                section === i.id
                  ? 'bg-text/[0.08] text-text [&>svg]:text-accent-text'
                  : 'text-muted hover:bg-text/[0.05] hover:text-text',
              )}
            >
              {i.icon}
              {i.label}
            </button>
          ))}
        </nav>
        <div className="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto px-4 pt-6 pb-8 sm:px-8">
          <div className="mx-auto flex w-full max-w-3xl flex-col gap-3">
            {section === 'appearance' && <Appearance />}
            {section === 'terminal' && <Terminal />}
            {section === 'security' && <Security status={status} onStatus={onStatus} />}
            {section === 'sync' && <SyncSettings />}
            {section === 'ai' && <AiSettingsCard />}
            {section === 'agents' && <AgentSettingsCard />}
            {section === 'shortcuts' && <Shortcuts />}
            {section === 'about' && <About />}
            <p className="text-subtle flex items-center gap-1.5 pt-2 text-xs">
              <Monitor className="size-3.5" />
              {t('settings.storedEncrypted')}
            </p>
          </div>
        </div>
      </div>
    </div>
  );
}
