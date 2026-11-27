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
    <section
      className={cn(
        'bg-surface-2 border-border rounded-[var(--radius-card)] border p-4 shadow-xs',
        className,
      )}
    >
      {title ? (
        <header className="mb-3 flex items-center justify-between gap-2">
          <h3 className="text-[14px] font-semibold">{title}</h3>
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
    <div className="flex flex-col items-center justify-center gap-4 px-6 py-16 text-center">
      <div className="bg-surface-2 border-border text-muted relative flex size-14 items-center justify-center rounded-2xl border shadow-xs [&>svg]:size-6">
        {icon}
      </div>
      <div>
        <p className="text-[15px] font-semibold">{title}</p>
        {description ? (
          <p className="text-muted mx-auto mt-1 max-w-sm text-[13px] text-pretty">{description}</p>
        ) : null}
      </div>
      {action}
    </div>
  );
}

export function SectionHeader({ children, actions }: { children: ReactNode; actions?: ReactNode }) {
  return (
    <div className="mt-7 mb-3 flex items-center justify-between first:mt-0">
      <h2 className="text-subtle text-[11px] font-semibold tracking-[0.08em] uppercase">
        {children}
      </h2>
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
        <div className="text-[13px] font-medium">{label}</div>
        {description ? <div className="text-muted mt-0.5 text-xs">{description}</div> : null}
      </div>
      <div className="shrink-0">{children}</div>
    </div>
  );
}

/** Title row at the top of a vault section: heading, optional description and actions. */
export function PageHeader({
  title,
  description,
  actions,
  className,
}: {
  title: ReactNode;
  description?: ReactNode;
  actions?: ReactNode;
  className?: string;
}) {
  return (
    <header className={cn('flex flex-wrap items-end justify-between gap-x-4 gap-y-3', className)}>
      <div className="min-w-0">
        <h1 className="text-[22px] leading-tight font-semibold tracking-tight">{title}</h1>
        {description ? <p className="text-muted mt-1 text-[13px]">{description}</p> : null}
      </div>
      {actions ? <div className="flex flex-wrap items-center gap-2">{actions}</div> : null}
    </header>
  );
}
