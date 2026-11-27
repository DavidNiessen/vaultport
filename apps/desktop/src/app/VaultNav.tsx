// SPDX-License-Identifier: GPL-3.0-or-later
import {
  ArrowRightLeft,
  BookKey,
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
import { useApp, type VaultSection } from '@/stores/app';

export const SECTIONS: { id: VaultSection; icon: ReactNode; key: string }[] = [
  { id: 'hosts', icon: <Server />, key: 'nav.hosts' },
  { id: 'keychain', icon: <KeyRound />, key: 'nav.keychain' },
  { id: 'forwarding', icon: <ArrowRightLeft />, key: 'nav.forwarding' },
  { id: 'snippets', icon: <Code2 />, key: 'nav.snippets' },
  { id: 'knownHosts', icon: <Fingerprint />, key: 'nav.knownHosts' },
  { id: 'logs', icon: <History />, key: 'nav.logs' },
];

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
  const item = (id: VaultSection, icon: ReactNode, label: string) => {
    const active = section === id;
    const btn = (
      <button
        key={id}
        type="button"
        aria-current={active ? 'page' : undefined}
        aria-label={compact ? label : undefined}
        onClick={() => setSection(id)}
        className={cn(
          'flex h-9 items-center gap-3 rounded-[10px] text-[13px] font-medium transition-colors [&>svg]:size-[18px]',
          compact ? 'w-10 justify-center' : 'px-3',
          active ? 'bg-surface-3 text-text' : 'text-muted hover:bg-surface-2 hover:text-text',
        )}
      >
        {icon}
        {compact ? null : <span>{label}</span>}
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
  return (
    <nav
      aria-label={t('nav.label')}
      className={cn(
        'border-border bg-surface-1/60 flex shrink-0 flex-col gap-1 border-r py-3',
        compact ? 'w-14 items-center' : 'w-52 px-2.5',
      )}
    >
      {!compact ? (
        <div className="text-subtle mb-2 flex items-center gap-2 px-3 text-[11px] font-semibold tracking-wide uppercase">
          <BookKey className="size-3.5" />
          <span className="truncate">{vaultName}</span>
        </div>
      ) : null}
      {SECTIONS.map((s) => item(s.id, s.icon, t(s.key)))}
      <div className="flex-1" />
      {item('settings', <Settings />, t('nav.settings'))}
      <Tooltip content={t('lock.lockNow')} side="right">
        <button
          type="button"
          onClick={onLock}
          aria-label={t('lock.lockNow')}
          className={cn(
            'text-muted hover:bg-surface-2 hover:text-text flex h-9 items-center gap-3 rounded-[10px] text-[13px] font-medium [&>svg]:size-[18px]',
            compact ? 'w-10 justify-center' : 'px-3',
          )}
        >
          <Lock />
          {compact ? null : <span>{t('lock.lockNow')}</span>}
        </button>
      </Tooltip>
    </nav>
  );
}
