// SPDX-License-Identifier: GPL-3.0-or-later
/**
 * The app mark, drawn inline so it takes the colours of the active colour preset: the tile is
 * a gradient of the accent and the prompt glyph uses the colour meant for text on the accent.
 * Same geometry as public/logo.svg.
 */
import { useId } from 'react';

import { cn } from '@/lib/cn';

export function Logo({ className, title }: { className?: string; title?: string }) {
  const id = useId();
  return (
    <svg
      viewBox="64 64 896 896"
      className={cn('shrink-0', className)}
      role={title ? 'img' : undefined}
      aria-label={title}
      aria-hidden={title ? undefined : true}
    >
      <defs>
        <linearGradient id={`${id}-bg`} x1="0" y1="0" x2="1" y2="1">
          <stop offset="0" stopColor="color-mix(in oklab, var(--accent) 70%, white)" />
          <stop offset="0.55" stopColor="var(--accent)" />
          <stop offset="1" stopColor="color-mix(in oklab, var(--accent) 70%, black)" />
        </linearGradient>
      </defs>
      <rect x="64" y="64" width="896" height="896" rx="208" fill={`url(#${id}-bg)`} />
      <path
        d="M292 380 L452 512 L292 644"
        fill="none"
        stroke="var(--accent-fg)"
        strokeWidth="84"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
      <rect x="516" y="604" width="236" height="80" rx="40" fill="var(--accent-fg)" />
      <circle cx="736" cy="340" r="44" fill="var(--accent-fg)" opacity="0.55" />
    </svg>
  );
}
