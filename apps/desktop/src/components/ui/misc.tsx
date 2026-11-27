// SPDX-License-Identifier: GPL-3.0-or-later
import type { ReactNode } from 'react';

import { cn } from '@/lib/cn';

export function Card({
  children,
  className,
  title,
  actions,
}: {
  children: ReactNode;
  className?: string;
  title?: ReactNode;
  actions?: ReactNode;
}) {
  return (
    <section className={cn('bg-surface-2 rounded-[var(--radius-card)] p-4', className)}>
      {title ? (
        <header className="mb-3 flex items-center justify-between gap-2">
          <h3 className="text-[13px] font-semibold">{title}</h3>
          {actions}
        </header>
      ) : null}
      {children}
    </section>
  );
}

export function EmptyState({
  icon,
  title,
  description,
  action,
}: {
  icon: ReactNode;
  title: ReactNode;
  description?: ReactNode;
  action?: ReactNode;
}) {
  return (
    <div className="flex flex-col items-center justify-center gap-3 px-6 py-16 text-center">
      <div className="bg-surface-2 text-muted flex size-14 items-center justify-center rounded-2xl [&>svg]:size-6">
        {icon}
      </div>
      <div>
        <p className="font-semibold">{title}</p>
        {description ? <p className="text-muted mt-1 max-w-sm text-[13px]">{description}</p> : null}
      </div>
      {action}
    </div>
  );
}

export function SectionHeader({ children, actions }: { children: ReactNode; actions?: ReactNode }) {
  return (
    <div className="mt-6 mb-2.5 flex items-center justify-between first:mt-0">
      <h2 className="text-muted text-[12px] font-semibold tracking-wide uppercase">{children}</h2>
      {actions}
    </div>
  );
}

/** A label/control row used in settings and inspectors. */
export function Row({
  label,
  description,
  children,
}: {
  label: ReactNode;
  description?: ReactNode;
  children: ReactNode;
}) {
  return (
    <div className="flex min-h-11 items-center justify-between gap-4 py-1.5">
      <div className="min-w-0">
        <div className="text-[13px]">{label}</div>
        {description ? <div className="text-subtle text-xs">{description}</div> : null}
      </div>
      <div className="shrink-0">{children}</div>
    </div>
  );
}
