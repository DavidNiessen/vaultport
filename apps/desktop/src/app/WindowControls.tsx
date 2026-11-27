// SPDX-License-Identifier: GPL-3.0-or-later
/**
 * Window caption buttons drawn by the app on Windows and Linux (macOS keeps its native traffic
 * lights). Windows follows Fluent / Windows 11 caption metrics; Linux follows libadwaita and
 * honours the user's `button-layout` setting (order and side).
 */
import { getCurrentWindow } from '@tauri-apps/api/window';
import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { isTauri, type PlatformInfo } from '@/lib/ipc';
import { cn } from '@/lib/cn';

type Action = 'minimize' | 'maximize' | 'close';

function useMaximized(): boolean {
  const [max, setMax] = useState(false);
  useEffect(() => {
    if (!isTauri()) return;
    const w = getCurrentWindow();
    let unlisten: (() => void) | undefined;
    const refresh = () => {
      void w.isMaximized().then(setMax);
    };
    refresh();
    void w.onResized(refresh).then((u) => (unlisten = u));
    return () => unlisten?.();
  }, []);
  return max;
}

function run(action: Action) {
  const w = getCurrentWindow();
  switch (action) {
    case 'minimize':
      void w.minimize();
      break;
    case 'maximize':
      void w.toggleMaximize();
      break;
    case 'close':
      void w.close();
      break;
  }
}

/** Parses a GTK/GNOME `button-layout` string into left and right button lists. */
export function parseLayout(layout: string | null): { left: Action[]; right: Action[] } {
  const raw = layout ?? 'appmenu:minimize,maximize,close';
  const [l = '', r = ''] = raw.split(':');
  const pick = (s: string) =>
    s
      .split(',')
      .map((x) => x.trim())
      .filter((x): x is Action => x === 'minimize' || x === 'maximize' || x === 'close');
  return { left: pick(l), right: pick(r) };
}

function WindowsGlyph({ action, maximized }: { action: Action; maximized: boolean }) {
  const common = { width: 10, height: 10, viewBox: '0 0 10 10', 'aria-hidden': true } as const;
  if (action === 'minimize')
    return (
      <svg {...common}>
        <path d="M0 5h10" stroke="currentColor" strokeWidth="1" />
      </svg>
    );
  if (action === 'close')
    return (
      <svg {...common}>
        <path d="M0 0l10 10M10 0L0 10" stroke="currentColor" strokeWidth="1" />
      </svg>
    );
  return maximized ? (
    <svg {...common}>
      <path d="M2 0h8v8M0 2h8v8H0z" fill="none" stroke="currentColor" strokeWidth="1" />
    </svg>
  ) : (
    <svg {...common}>
      <rect
        x="0.5"
        y="0.5"
        width="9"
        height="9"
        fill="none"
        stroke="currentColor"
        strokeWidth="1"
      />
    </svg>
  );
}

export function WindowsControls() {
  const { t } = useTranslation();
  const maximized = useMaximized();
  const actions: Action[] = ['minimize', 'maximize', 'close'];
  return (
    <div className="no-drag flex h-full items-stretch self-stretch">
      {actions.map((a) => (
        <button
          key={a}
          type="button"
          aria-label={t(`window.${a === 'maximize' && maximized ? 'restore' : a}`)}
          onClick={() => run(a)}
          className={cn(
            'text-text flex w-[46px] items-center justify-center transition-colors',
            a === 'close' ? 'hover:bg-[#c42b1c] hover:text-white' : 'hover:bg-surface-3',
          )}
        >
          <WindowsGlyph action={a} maximized={maximized} />
        </button>
      ))}
    </div>
  );
}

function LinuxGlyph({ action, maximized }: { action: Action; maximized: boolean }) {
  const common = { width: 16, height: 16, viewBox: '0 0 16 16', 'aria-hidden': true } as const;
  if (action === 'minimize')
    return (
      <svg {...common}>
        <path d="M4.5 11.5h7" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
      </svg>
    );
  if (action === 'close')
    return (
      <svg {...common}>
        <path
          d="M5 5l6 6M11 5l-6 6"
          stroke="currentColor"
          strokeWidth="1.5"
          strokeLinecap="round"
        />
      </svg>
    );
  return maximized ? (
    <svg {...common}>
      <rect
        x="5"
        y="5"
        width="6"
        height="6"
        rx="1"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.5"
      />
    </svg>
  ) : (
    <svg {...common}>
      <rect
        x="4"
        y="4"
        width="8"
        height="8"
        rx="1.5"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.5"
      />
    </svg>
  );
}

export function LinuxControls({ actions }: { actions: Action[] }) {
  const { t } = useTranslation();
  const maximized = useMaximized();
  if (actions.length === 0) return null;
  return (
    <div className="no-drag flex items-center gap-2 px-2.5">
      {actions.map((a) => (
        <button
          key={a}
          type="button"
          aria-label={t(`window.${a === 'maximize' && maximized ? 'restore' : a}`)}
          onClick={() => run(a)}
          className="bg-surface-3 text-text hover:bg-surface-4 flex size-6 items-center justify-center rounded-full transition-colors"
        >
          <LinuxGlyph action={a} maximized={maximized} />
        </button>
      ))}
    </div>
  );
}

/** Returns the controls to render at the start and end of the title bar. */
export function useWindowControls(p: PlatformInfo): {
  start: Action[];
  end: Action[];
  kind: 'windows' | 'linux' | 'none';
} {
  if (p.mobile || p.nativeDecorations || p.kind === 'macos')
    return { start: [], end: [], kind: 'none' };
  if (p.kind === 'windows')
    return { start: [], end: ['minimize', 'maximize', 'close'], kind: 'windows' };
  const { left, right } = parseLayout(p.decorationLayout);
  return { start: left, end: right, kind: 'linux' };
}
