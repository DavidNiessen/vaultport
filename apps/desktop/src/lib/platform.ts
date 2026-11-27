// SPDX-License-Identifier: GPL-3.0-or-later
import { useQuery } from '@tanstack/react-query';
import { useEffect, useState, useSyncExternalStore } from 'react';

import { commands, type PlatformInfo } from './ipc';

const FALLBACK: PlatformInfo = {
  kind: 'linux',
  mobile: false,
  arch: 'unknown',
  version: '0.0.0',
  decorationLayout: null,
  nativeDecorations: false,
  localTerminal: false,
};

export function usePlatform(): PlatformInfo {
  const { data } = useQuery({
    queryKey: ['platform'],
    queryFn: () => commands.platformInfo(),
    staleTime: Infinity,
  });
  return data ?? FALLBACK;
}

/** `true` on macOS/iOS where the primary modifier is ⌘. */
export function isApple(p: PlatformInfo): boolean {
  return p.kind === 'macos' || p.kind === 'ios';
}

/** Formats a shortcut like `Mod+K` for display on the current platform. */
export function formatShortcut(p: PlatformInfo, shortcut: string): string {
  const apple = isApple(p);
  return shortcut
    .split('+')
    .map((k) => {
      switch (k) {
        case 'Mod':
          return apple ? '⌘' : 'Ctrl';
        case 'Alt':
          return apple ? '⌥' : 'Alt';
        case 'Shift':
          return apple ? '⇧' : 'Shift';
        case 'Ctrl':
          return apple ? '⌃' : 'Ctrl';
        default:
          return k;
      }
    })
    .join(apple ? '' : '+');
}

export type Breakpoint = 'phone' | 'tablet' | 'desktop';

const queries = {
  phone: '(max-width: 639.98px)',
  tablet: '(min-width: 640px) and (max-width: 1023.98px)',
};

function currentBreakpoint(): Breakpoint {
  if (typeof window === 'undefined') return 'desktop';
  if (window.matchMedia(queries.phone).matches) return 'phone';
  if (window.matchMedia(queries.tablet).matches) return 'tablet';
  return 'desktop';
}

function subscribe(cb: () => void): () => void {
  const mqs = Object.values(queries).map((q) => window.matchMedia(q));
  mqs.forEach((m) => {
    m.addEventListener('change', cb);
  });
  return () => {
    mqs.forEach((m) => {
      m.removeEventListener('change', cb);
    });
  };
}

/** Responsive breakpoint: phone (< 640px), tablet (640–1023px), desktop (≥ 1024px). */
export function useBreakpoint(): Breakpoint {
  return useSyncExternalStore(subscribe, currentBreakpoint, () => 'desktop');
}

/** Resolves the effective color scheme for a theme preference. */
export function useColorScheme(pref: 'system' | 'dark' | 'light'): 'dark' | 'light' {
  const [system, setSystem] = useState<'dark' | 'light'>(() =>
    typeof window !== 'undefined' && window.matchMedia('(prefers-color-scheme: light)').matches
      ? 'light'
      : 'dark',
  );
  useEffect(() => {
    const mq = window.matchMedia('(prefers-color-scheme: light)');
    const on = () => {
      setSystem(mq.matches ? 'light' : 'dark');
    };
    mq.addEventListener('change', on);
    return () => {
      mq.removeEventListener('change', on);
    };
  }, []);
  return pref === 'system' ? system : pref;
}
