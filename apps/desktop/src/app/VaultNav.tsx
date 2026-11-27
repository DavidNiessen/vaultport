// SPDX-License-Identifier: GPL-3.0-or-later
import {
  ArrowRightLeft,
  Code2,
  Fingerprint,
  History,
  KeyRound,
  Lock,
  Server,
  Settings,
} from 'lucide-react';
import type { ReactNode } from 'react';
import { useTranslation } from 'react-i18next';

import { Tooltip } from '@/components/ui/controls';
import { cn } from '@/lib/cn';
import { useForwards, useHosts, useKeys, useKnownHosts, useSnippets } from '@/lib/queries';
import { useApp, type VaultSection } from '@/stores/app';

export const SECTIONS: { id: VaultSection; icon: ReactNode; key: string }[] = [
  { id: 'hosts', icon: <Server />, key: 'nav.hosts' },
  { id: 'keychain', icon: <KeyRound />, key: 'nav.keychain' },
  { id: 'forwarding', icon: <ArrowRightLeft />, key: 'nav.forwarding' },
  { id: 'snippets', icon: <Code2 />, key: 'nav.snippets' },
  { id: 'knownHosts', icon: <Fingerprint />, key: 'nav.knownHosts' },
  { id: 'logs', icon: <History />, key: 'nav.logs' },
];

/** Item counts shown next to each section, from the already-cached vault lists. */
function useSectionCounts(): Partial<Record<VaultSection, number>> {
  const hosts = useHosts().data?.length;
  const keys = useKeys().data?.length;
  const forwards = useForwards().data?.length;
  const snippets = useSnippets().data?.length;
  const knownHosts = useKnownHosts().data?.length;
  return { hosts, keychain: keys, forwarding: forwards, snippets, knownHosts };
}

/** The vault this window has open, with a reminder that it is unlocked. */
function VaultBadge({ name, compact }: { name: string; compact: boolean }) {
  const { t } = useTranslation();
  if (compact) {
    return (
      <Tooltip content={`${name} · ${t('nav.unlocked')}`} side="right">
        <div className="relative mb-3">
          <img src="/logo.svg" alt={name} className="size-8" />
          <span
            className="bg-success ring-sidebar absolute -right-0.5 -bottom-0.5 size-2.5 rounded-full ring-2"
            aria-hidden
          />
        </div>
      </Tooltip>
    );
  }
  return (
    <div className="mb-4 flex items-center gap-2.5 px-2">
      <img src="/logo.svg" alt="" className="size-8 shrink-0" />
      <div className="min-w-0">
        <div className="truncate text-[13px] leading-tight font-semibold">{name}</div>
        <div className="text-muted mt-0.5 flex items-center gap-1.5 text-[11px] leading-tight">
          <span className="bg-success size-1.5 rounded-full" aria-hidden />
          {t('nav.unlocked')}
        </div>
      </div>
    </div>
  );
}

/** Left navigation of the Vault tab: labelled on desktop, icon rail on tablets. */
export function VaultNav({
  compact,
  vaultName,
  onLock,
}: {
  compact: boolean;
  vaultName: string;
  onLock: () => void;
}) {
  const { t } = useTranslation();
  const { section, setSection } = useApp();
  const counts = useSectionCounts();
  const itemClass = (active: boolean) =>
    cn(
      'group relative flex h-8 items-center gap-2.5 rounded-[7px] text-[13px] font-medium transition-colors duration-150 pointer-coarse:h-11 [&>svg]:size-4 [&>svg]:shrink-0',
      compact ? 'w-9 justify-center pointer-coarse:w-11' : 'px-2.5',
      active
        ? 'bg-surface-3 text-text shadow-[0_0_0_1px_var(--border)] [&>svg]:text-accent-text'
        : 'text-muted hover:bg-surface-2 hover:text-text',
    );
  const item = (id: VaultSection, icon: ReactNode, label: string) => {
    const active = section === id;
    const count = counts[id];
    const btn = (
      <button
        key={id}
        type="button"
        aria-current={active ? 'page' : undefined}
        aria-label={compact ? label : undefined}
        onClick={() => setSection(id)}
        className={itemClass(active)}
      >
        {active && !compact ? (
          <span className="bg-accent absolute top-1.5 bottom-1.5 -left-2.5 w-[3px] rounded-r-full" />
        ) : null}
        {icon}
        {compact ? null : (
          <>
            <span className="min-w-0 flex-1 truncate text-left">{label}</span>
            {count ? (
              <span className="text-subtle font-mono text-[11px] tabular-nums">{count}</span>
            ) : null}
          </>
        )}
      </button>
    );
    return compact ? (
      <Tooltip key={id} content={label} side="right">
        {btn}
      </Tooltip>
    ) : (
      btn
    );
  };
  const lock = (
    <button
      type="button"
      onClick={onLock}
      aria-label={compact ? t('lock.lockNow') : undefined}
      className={itemClass(false)}
    >
      <Lock />
      {compact ? null : <span className="flex-1 text-left">{t('lock.lockNow')}</span>}
    </button>
  );
  return (
    <nav
      aria-label={t('nav.label')}
      className={cn(
        'border-border bg-sidebar flex shrink-0 flex-col gap-0.5 border-r pt-4 pb-3',
        compact ? 'w-[60px] items-center' : 'w-56 px-2.5',
      )}
    >
      <VaultBadge name={vaultName} compact={compact} />
      {!compact ? (
        <div className="text-subtle mb-1 px-2.5 text-[11px] font-semibold tracking-[0.08em] uppercase">
          {t('nav.vault')}
        </div>
      ) : null}
      {SECTIONS.map((s) => item(s.id, s.icon, t(s.key)))}
      <div className="flex-1" />
      <div
        className={cn(
          'border-border mt-2 flex flex-col gap-0.5 border-t pt-2',
          compact && 'items-center',
        )}
      >
        {item('settings', <Settings />, t('nav.settings'))}
        {compact ? (
          <Tooltip content={t('lock.lockNow')} side="right">
            {lock}
          </Tooltip>
        ) : (
          lock
        )}
      </div>
    </nav>
  );
}
