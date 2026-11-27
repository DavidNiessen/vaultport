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

import { Logo } from '@/components/Logo';
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
          <Logo title={name} className="size-8" />
          <span
            className="bg-success ring-sidebar absolute -right-0.5 -bottom-0.5 size-2.5 rounded-full ring-2"
            aria-hidden
          />
        </div>
      </Tooltip>
    );
  }
  return (
    <div className="bg-text/[0.06] mb-5 flex items-center gap-2.5 rounded-[14px] p-2.5">
      <Logo className="size-8" />
      <div className="min-w-0">
        <div className="truncate text-[14px] leading-tight font-extrabold">{name}</div>
        <div className="text-muted mt-0.5 flex items-center gap-1.5 text-[11.5px] leading-tight font-semibold">
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
      'group relative flex h-9 items-center gap-3 rounded-[10px] text-[14px] font-bold transition-colors duration-150 pointer-coarse:h-11 [&>svg]:size-[18px] [&>svg]:shrink-0',
      compact ? 'w-10 justify-center pointer-coarse:w-11' : 'px-3',
      active
        ? 'bg-text/[0.08] text-text [&>svg]:text-accent-text'
        : 'text-text/80 hover:bg-text/[0.05] hover:text-text',
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
        {icon}
        {compact ? null : (
          <>
            <span className="min-w-0 flex-1 truncate text-left">{label}</span>
            {count ? (
              <span className="text-subtle text-[12px] font-bold tabular-nums">{count}</span>
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
        'border-border bg-sidebar/70 flex shrink-0 flex-col gap-1 border-r pt-3 pb-3',
        compact ? 'w-[64px] items-center' : 'w-60 px-3',
      )}
    >
      <VaultBadge name={vaultName} compact={compact} />
      {!compact ? (
        <div className="text-subtle mb-1 px-3 text-[11px] font-extrabold tracking-[0.1em] uppercase">
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
