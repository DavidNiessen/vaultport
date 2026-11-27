// SPDX-License-Identifier: GPL-3.0-or-later
import { QueryClientProvider, useQuery, useQueryClient } from '@tanstack/react-query';
import { useCallback, useEffect, useRef, useState } from 'react';
import { Toaster } from 'sonner';

import { TooltipProvider } from '@/components/ui/controls';
import { Spinner } from '@/components/ui/controls';
import { commands, onVaultLocked, type VaultStatus } from '@/lib/ipc';
import { useColorScheme, usePlatform } from '@/lib/platform';
import { queryClient, useSettings, useVaultEvents } from '@/lib/queries';
import { useApp } from '@/stores/app';

import { DEFAULT_SECURITY_SETTINGS, type SecuritySettings } from '../features/settings/security';
import { LockScreen } from '../features/vault-lock/LockScreen';
import { Shell } from './Shell';

/** Applies theme, accent and platform attributes to the document root. */
function useDocumentTheme() {
  const theme = useApp((s) => s.theme);
  const accent = useApp((s) => s.accent);
  const scheme = useColorScheme(theme);
  const platform = usePlatform();
  useEffect(() => {
    const root = document.documentElement;
    root.dataset.theme = scheme;
    root.dataset.platform = platform.kind;
    root.style.setProperty('--accent', accent);
    root.style.setProperty('--accent-hover', `color-mix(in oklab, ${accent} 88%, white)`);
    root.style.setProperty('--accent-soft', `color-mix(in oklab, ${accent} 16%, transparent)`);
  }, [scheme, accent, platform.kind]);
  return scheme;
}

/**
 * Locks the vault after inactivity and when the app goes to the background (mobile). Locking
 * on sleep happens in the core (`sleep.rs`).
 */
function useAutoLock(enabled: boolean, lock: () => void) {
  const { settings } = useSettings<SecuritySettings>('security', DEFAULT_SECURITY_SETTINGS);
  const platform = usePlatform();
  const last = useRef(0);
  useEffect(() => {
    if (!enabled) return;
    last.current = Date.now();
    const bump = () => {
      last.current = Date.now();
    };
    const events = ['keydown', 'pointerdown', 'wheel', 'touchstart'] as const;
    events.forEach((e) => window.addEventListener(e, bump, { passive: true }));
    const timer = setInterval(() => {
      if (
        settings.autoLockMinutes > 0 &&
        Date.now() - last.current > settings.autoLockMinutes * 60_000
      )
        lock();
    }, 15_000);
    const onVisibility = () => {
      if (document.hidden && platform.mobile && settings.lockOnBackground) lock();
    };
    document.addEventListener('visibilitychange', onVisibility);
    return () => {
      events.forEach((e) => window.removeEventListener(e, bump));
      clearInterval(timer);
      document.removeEventListener('visibilitychange', onVisibility);
    };
  }, [enabled, lock, settings, platform.mobile]);
}

function Root() {
  const qc = useQueryClient();
  const scheme = useDocumentTheme();
  const initial = useQuery({ queryKey: ['vaultStatus'], queryFn: () => commands.vaultStatus() });
  const [status, setStatus] = useState<VaultStatus | null>(null);
  const current = status ?? initial.data ?? null;
  const unlocked = Boolean(current && !current.locked && current.active);
  useVaultEvents(unlocked);

  const onStatus = useCallback(
    (s: VaultStatus) => {
      setStatus(s);
      if (s.locked) {
        qc.removeQueries({
          predicate: (q) => q.queryKey[0] !== 'platform' && q.queryKey[0] !== 'vaultStatus',
        });
        useApp.setState({ tabs: [], activeTab: 'vault', selected: null });
      } else {
        void qc.invalidateQueries();
      }
    },
    [qc],
  );
  const lock = useCallback(() => {
    void commands.vaultLock().then(onStatus);
  }, [onStatus]);
  useAutoLock(unlocked, lock);

  // The core locks the vault by itself when the computer wakes from sleep.
  useEffect(() => {
    if (!unlocked) return;
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    void onVaultLocked(() => {
      void commands.vaultStatus().then(onStatus);
    }).then((u) => {
      if (cancelled) u();
      else unlisten = u;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [unlocked, onStatus]);

  // Blur the window content when it loses focus on mobile (app switcher privacy).
  const [hidden, setHidden] = useState(false);
  useEffect(() => {
    const on = () => setHidden(document.hidden);
    document.addEventListener('visibilitychange', on);
    return () => document.removeEventListener('visibilitychange', on);
  }, []);

  return (
    <>
      <div className={hidden ? 'h-full blur-xl' : 'h-full'}>
        {!current ? (
          <div className="flex h-full items-center justify-center">
            <Spinner />
          </div>
        ) : unlocked ? (
          <Shell status={current} onStatus={onStatus} onLock={lock} />
        ) : (
          <LockScreen status={current} onChange={onStatus} />
        )}
      </div>
      <Toaster
        theme={scheme}
        position="bottom-right"
        richColors
        closeButton
        toastOptions={{ className: 'font-sans' }}
      />
    </>
  );
}

export function App() {
  return (
    <QueryClientProvider client={queryClient}>
      <TooltipProvider>
        <Root />
      </TooltipProvider>
    </QueryClientProvider>
  );
}
