// SPDX-License-Identifier: GPL-3.0-or-later
/**
 * The app mark, drawn inline so it takes the colours of the active colour preset: an ink tile
 * lit by the preset's two glow colours, a prompt chevron and a glowing pill cursor. Same
 * geometry as public/logo.svg (which uses the default Iris colours).
 */
import { useId } from 'react';

import { cn } from '@/lib/cn';

export function Logo({ className, title }: { className?: string; title?: string }) {
  const id = useId();
  const glow1 = 'rgb(var(--glow-1))';
  const glow2 = 'rgb(var(--glow-2))';
  return (
    <svg
      viewBox="64 64 896 896"
      className={cn('shrink-0', className)}
      role={title ? 'img' : undefined}
      aria-label={title}
      aria-hidden={title ? undefined : true}
    >
      <defs>
        <linearGradient id={`${id}-tile`} x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stopColor="#1c1930" />
          <stop offset="1" stopColor="#0d0c12" />
        </linearGradient>
        <radialGradient id={`${id}-a`} cx="0.3" cy="0.26" r="0.62">
          <stop offset="0" style={{ stopColor: glow1, stopOpacity: 0.85 }} />
          <stop offset="0.55" style={{ stopColor: glow1, stopOpacity: 0.28 }} />
          <stop offset="1" style={{ stopColor: glow1, stopOpacity: 0 }} />
        </radialGradient>
        <radialGradient id={`${id}-b`} cx="0.82" cy="0.86" r="0.55">
          <stop offset="0" style={{ stopColor: glow2, stopOpacity: 0.7 }} />
          <stop offset="1" style={{ stopColor: glow2, stopOpacity: 0 }} />
        </radialGradient>
        <linearGradient id={`${id}-pill`} x1="0" y1="0" x2="1" y2="0">
          <stop offset="0" style={{ stopColor: `color-mix(in oklab, ${glow1} 45%, white)` }} />
          <stop offset="1" style={{ stopColor: `color-mix(in oklab, ${glow1} 85%, white)` }} />
        </linearGradient>
        <clipPath id={`${id}-clip`}>
          <rect x="64" y="64" width="896" height="896" rx="232" />
        </clipPath>
      </defs>
      <g clipPath={`url(#${id}-clip)`}>
        <rect x="64" y="64" width="896" height="896" fill={`url(#${id}-tile)`} />
        <rect x="64" y="64" width="896" height="896" fill={`url(#${id}-a)`} />
        <rect x="64" y="64" width="896" height="896" fill={`url(#${id}-b)`} />
      </g>
      <rect
        x="66"
        y="66"
        width="892"
        height="892"
        rx="230"
        fill="none"
        stroke="#ffffff"
        strokeOpacity="0.1"
        strokeWidth="4"
      />
      <path
        d="M286 352 L462 512 L286 672"
        fill="none"
        stroke="#ffffff"
        strokeWidth="112"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
      <rect x="530" y="606" width="236" height="112" rx="56" fill={`url(#${id}-pill)`} />
    </svg>
  );
}
