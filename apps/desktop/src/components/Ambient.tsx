// SPDX-License-Identifier: GPL-3.0-or-later
/**
 * The living background: a few soft, elliptical lights in the colours of the active preset.
 * Each light has two layers: the outer one wanders along its own path, the inner one slowly
 * turns and stretches. Every light uses different durations, so the overlapping shapes keep
 * merging and changing instead of visibly looping. Only `transform` is animated, which keeps
 * the work on the compositor. Styles live in styles/index.css (`.ambient`).
 */
import type { CSSProperties } from 'react';

import { cn } from '@/lib/cn';

interface Blob {
  /** Position and size of the light (any CSS lengths). */
  box: CSSProperties;
  /** Which preset glow colour, and how strong relative to --glow-strength. */
  tone: 1 | 2;
  strength: number;
  /** Durations in seconds of the wander path and of the turn/stretch cycle. */
  wander: number;
  morph: number;
  /** Which of the three wander paths to follow. */
  path: 1 | 2 | 3;
  /** Start part-way through the cycles so the lights don't move in step. */
  delay: number;
}

// A glow along the top edge of the window, behind the app.
const TOP: Blob[] = [
  {
    box: { top: '-300px', left: '-12vw', width: '62vw', height: '520px' },
    tone: 1,
    strength: 1.1,
    wander: 31,
    morph: 23,
    path: 1,
    delay: -4,
  },
  {
    box: { top: '-280px', right: '-14vw', width: '52vw', height: '460px' },
    tone: 2,
    strength: 0.8,
    wander: 37,
    morph: 29,
    path: 2,
    delay: -11,
  },
  {
    box: { top: '-320px', left: '26vw', width: '38vw', height: '420px' },
    tone: 1,
    strength: 0.55,
    wander: 43,
    morph: 19,
    path: 3,
    delay: -20,
  },
];

// The whole screen, behind the lock screen.
const FULL: Blob[] = [
  {
    box: { top: '-22vh', left: '-16vw', width: '72vw', height: '78vh' },
    tone: 1,
    strength: 1.6,
    wander: 27,
    morph: 21,
    path: 1,
    delay: -3,
  },
  {
    box: { bottom: '-28vh', right: '-18vw', width: '68vw', height: '80vh' },
    tone: 2,
    strength: 1.4,
    wander: 33,
    morph: 26,
    path: 2,
    delay: -9,
  },
  {
    box: { top: '22vh', left: '28vw', width: '42vw', height: '50vh' },
    tone: 1,
    strength: 0.7,
    wander: 41,
    morph: 17,
    path: 3,
    delay: -15,
  },
  {
    box: { top: '-10vh', right: '4vw', width: '34vw', height: '42vh' },
    tone: 2,
    strength: 0.8,
    wander: 47,
    morph: 31,
    path: 1,
    delay: -24,
  },
];

export function Ambient({ variant }: { variant: 'top' | 'full' }) {
  const blobs = variant === 'full' ? FULL : TOP;
  return (
    <div className={cn('ambient', variant === 'full' && 'ambient-full')} aria-hidden>
      {blobs.map((b, i) => (
        <div
          key={i}
          className={`ambient-wander ambient-path-${String(b.path)}`}
          style={{
            ...b.box,
            animationDuration: `${String(b.wander)}s`,
            animationDelay: `${String(b.delay)}s`,
          }}
        >
          <div
            className="ambient-blob"
            style={
              {
                '--blob-color': `rgb(var(--glow-${String(b.tone)}) / calc(var(--glow-strength) * ${String(b.strength)}))`,
                animationDuration: `${String(b.morph)}s`,
                animationDelay: `${String(b.delay / 2)}s`,
              } as CSSProperties
            }
          />
        </div>
      ))}
    </div>
  );
}
