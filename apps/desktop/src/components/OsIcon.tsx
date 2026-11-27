// SPDX-License-Identifier: GPL-3.0-or-later
/**
 * Rounded-square host tiles. We deliberately use monograms and neutral colors instead of
 * distribution logos (trademarks), but keep each OS recognisable by color.
 */
import { Folder, Server } from 'lucide-react';

import { cn } from '@/lib/cn';
import type { OsKind } from '@/lib/ipc';

const OS: Partial<Record<OsKind, { bg: string; label: string }>> = {
  ubuntu: { bg: '#E95420', label: 'Ub' },
  debian: { bg: '#A80030', label: 'De' },
  raspbian: { bg: '#C51A4A', label: 'Pi' },
  fedora: { bg: '#3C6EB4', label: 'Fe' },
  rhel: { bg: '#CC0000', label: 'RH' },
  centos: { bg: '#932279', label: 'Ce' },
  rocky: { bg: '#10B981', label: 'Ro' },
  alma: { bg: '#0F4266', label: 'Al' },
  arch: { bg: '#1793D1', label: 'Ar' },
  alpine: { bg: '#0D597F', label: 'Ap' },
  amazon: { bg: '#FF9900', label: 'Am' },
  suse: { bg: '#30BA78', label: 'Su' },
  nixos: { bg: '#5277C3', label: 'Nx' },
  freebsd: { bg: '#AB2B28', label: 'Fb' },
  openbsd: { bg: '#E6B800', label: 'Ob' },
  netbsd: { bg: '#F26711', label: 'Nb' },
  macos: { bg: '#6B7280', label: 'Mc' },
  windows: { bg: '#0078D4', label: 'Wi' },
  cisco: { bg: '#1BA0D7', label: 'Ci' },
  mikrotik: { bg: '#293239', label: 'Mt' },
  synology: { bg: '#4B4B4B', label: 'Sy' },
  truenas: { bg: '#0095D5', label: 'Tn' },
  proxmox: { bg: '#E57000', label: 'Px' },
  openwrt: { bg: '#00B5E2', label: 'Ow' },
  linux: { bg: '#334155', label: 'Lx' },
};

export function OsIcon({ os, size = 'md' }: { os: OsKind | undefined; size?: 'sm' | 'md' | 'lg' }) {
  const meta = os ? OS[os] : undefined;
  const dims =
    size === 'sm'
      ? 'size-7 text-[10px] rounded-lg'
      : size === 'lg'
        ? 'size-12 text-[15px] rounded-2xl'
        : 'size-9 text-[12px] rounded-[11px]';
  if (!meta) {
    return (
      <div
        className={cn('bg-surface-4 text-muted flex shrink-0 items-center justify-center', dims)}
        aria-hidden
      >
        <Server className={size === 'sm' ? 'size-3.5' : 'size-4'} />
      </div>
    );
  }
  return (
    <div
      className={cn('flex shrink-0 items-center justify-center font-bold text-white', dims)}
      style={{ backgroundColor: meta.bg }}
      aria-hidden
    >
      {meta.label}
    </div>
  );
}

export function GroupIcon({ size = 'md' }: { size?: 'sm' | 'md' }) {
  return (
    <div
      className={cn(
        'bg-accent-soft text-accent flex shrink-0 items-center justify-center',
        size === 'sm' ? 'size-7 rounded-lg' : 'size-9 rounded-[11px]',
      )}
      aria-hidden
    >
      <Folder className={size === 'sm' ? 'size-3.5' : 'size-4'} />
    </div>
  );
}
