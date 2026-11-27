// SPDX-License-Identifier: GPL-3.0-or-later
import * as SwitchPrimitive from '@radix-ui/react-switch';
import * as TooltipPrimitive from '@radix-ui/react-tooltip';
import { ChevronDown, X } from 'lucide-react';
import {
  type KeyboardEvent,
  type ReactElement,
  type ReactNode,
  type SelectHTMLAttributes,
  forwardRef,
  useState,
} from 'react';

import { cn } from '@/lib/cn';

export const Switch = forwardRef<
  HTMLButtonElement,
  SwitchPrimitive.SwitchProps & { label?: string }
>(function Switch({ className, label, ...props }, ref) {
  return (
    <SwitchPrimitive.Root
      ref={ref}
      aria-label={label}
      className={cn(
        'data-[state=checked]:bg-accent bg-surface-4 border-border-strong relative inline-flex h-[22px] w-10 shrink-0 cursor-pointer items-center rounded-full border transition-colors duration-150 disabled:opacity-50 data-[state=checked]:border-transparent',
        className,
      )}
      {...props}
    >
      <SwitchPrimitive.Thumb className="pointer-events-none block size-4 translate-x-[2px] rounded-full bg-white shadow-[0_1px_3px_rgb(0_0_0/0.35)] transition-transform duration-150 data-[state=checked]:translate-x-[20px]" />
    </SwitchPrimitive.Root>
  );
});

export const Select = forwardRef<HTMLSelectElement, SelectHTMLAttributes<HTMLSelectElement>>(
  function Select({ className, children, ...props }, ref) {
    return (
      <div className="relative">
        <select
          ref={ref}
          className={cn(
            'bg-surface-1 text-text border-border-input hover:border-border-strong focus:border-accent focus:ring-accent-soft h-9 w-full cursor-pointer appearance-none rounded-[var(--radius-control)] border pr-8 pl-3 text-[13px] shadow-xs transition-[border-color,box-shadow] duration-150 focus:ring-3 focus:outline-none pointer-coarse:h-11',
            className,
          )}
          {...props}
        >
          {children}
        </select>
        <ChevronDown className="text-subtle pointer-events-none absolute top-1/2 right-2.5 size-4 -translate-y-1/2" />
      </div>
    );
  },
);

interface SegmentedProps<T extends string> {
  value: T;
  onChange: (v: T) => void;
  options: { value: T; label: ReactNode; icon?: ReactNode }[];
  className?: string;
  ariaLabel: string;
}

export function Segmented<T extends string>({
  value,
  onChange,
  options,
  className,
  ariaLabel,
}: SegmentedProps<T>) {
  return (
    <div
      role="radiogroup"
      aria-label={ariaLabel}
      className={cn(
        'bg-surface-3 border-border inline-flex rounded-[var(--radius-control)] border p-0.5',
        className,
      )}
    >
      {options.map((o) => (
        <button
          key={o.value}
          type="button"
          role="radio"
          aria-checked={value === o.value}
          onClick={() => onChange(o.value)}
          className={cn(
            'flex h-7 items-center gap-1.5 rounded-[6px] px-3 text-[13px] font-medium transition-colors duration-150 pointer-coarse:h-10 [&>svg]:size-3.5',
            value === o.value
              ? 'bg-surface-1 text-text shadow-[0_1px_2px_rgb(0_0_0/0.15),0_0_0_1px_var(--border)]'
              : 'text-muted hover:text-text',
          )}
        >
          {o.icon}
          {o.label}
        </button>
      ))}
    </div>
  );
}

export function Badge({
  children,
  tone = 'neutral',
  className,
}: {
  children: ReactNode;
  tone?: 'neutral' | 'accent' | 'success' | 'warning' | 'danger' | 'info';
  className?: string;
}) {
  const tones = {
    neutral: 'bg-surface-3 text-muted border-border',
    accent: 'bg-accent-soft text-accent-text border-transparent',
    success: 'bg-success-soft text-success border-transparent',
    warning: 'bg-warning-soft text-warning border-transparent',
    danger: 'bg-danger-soft text-danger border-transparent',
    info: 'bg-info-soft text-info border-transparent',
  } as const;
  return (
    <span
      className={cn(
        'inline-flex h-5 items-center gap-1 rounded-md border px-1.5 text-[11px] font-medium whitespace-nowrap',
        tones[tone],
        className,
      )}
    >
      {children}
    </span>
  );
}

export function Kbd({ children }: { children: ReactNode }) {
  return (
    <kbd className="border-border-strong bg-surface-2 text-muted inline-flex h-5 min-w-5 items-center justify-center rounded-[5px] border border-b-2 px-1 font-sans text-[11px] leading-none">
      {children}
    </kbd>
  );
}

export function Tooltip({
  content,
  children,
  side = 'bottom',
}: {
  content: ReactNode;
  children: ReactElement;
  side?: 'top' | 'bottom' | 'left' | 'right';
}) {
  return (
    <TooltipPrimitive.Root delayDuration={400}>
      <TooltipPrimitive.Trigger asChild>{children}</TooltipPrimitive.Trigger>
      <TooltipPrimitive.Portal>
        <TooltipPrimitive.Content
          side={side}
          sideOffset={6}
          className="bg-surface-4 text-text animate-fade-in z-50 rounded-md px-2 py-1 text-xs font-medium shadow-[var(--shadow)]"
        >
          {content}
        </TooltipPrimitive.Content>
      </TooltipPrimitive.Portal>
    </TooltipPrimitive.Root>
  );
}

export const TooltipProvider = TooltipPrimitive.Provider;

/** Chip-style tag editor. Enter/comma adds, Backspace on empty input removes the last tag. */
export function TagInput({
  value,
  onChange,
  placeholder,
  suggestions = [],
  ariaLabel,
}: {
  value: string[];
  onChange: (tags: string[]) => void;
  placeholder?: string;
  suggestions?: string[];
  ariaLabel: string;
}) {
  const [draft, setDraft] = useState('');
  const add = (raw: string) => {
    const tag = raw.trim().replace(/,/g, '');
    if (!tag || value.some((t) => t.toLowerCase() === tag.toLowerCase())) return;
    onChange([...value, tag]);
  };
  const onKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'Enter' || e.key === ',') {
      e.preventDefault();
      add(draft);
      setDraft('');
    } else if (e.key === 'Backspace' && draft === '' && value.length > 0) {
      onChange(value.slice(0, -1));
    }
  };
  const listId = `tags-${ariaLabel.replace(/\s/g, '-')}`;
  return (
    <div className="bg-surface-1 border-border-input focus-within:border-accent focus-within:ring-accent-soft flex min-h-9 flex-wrap items-center gap-1.5 rounded-[var(--radius-control)] border px-2 py-1.5 shadow-xs transition-[border-color,box-shadow] duration-150 focus-within:ring-3">
      {value.map((t) => (
        <span
          key={t}
          className="bg-surface-3 text-text border-border inline-flex h-6 items-center gap-1 rounded-md border pr-1 pl-2 text-xs"
        >
          {t}
          <button
            type="button"
            className="text-subtle hover:text-danger rounded-full"
            onClick={() => onChange(value.filter((x) => x !== t))}
            aria-label={`Remove ${t}`}
          >
            <X className="size-3.5" />
          </button>
        </span>
      ))}
      <input
        className="text-text placeholder:text-subtle min-w-16 flex-1 bg-transparent text-sm outline-none"
        value={draft}
        aria-label={ariaLabel}
        placeholder={value.length === 0 ? placeholder : undefined}
        onChange={(e) => setDraft(e.target.value)}
        onKeyDown={onKeyDown}
        onBlur={() => {
          if (draft) {
            add(draft);
            setDraft('');
          }
        }}
        list={suggestions.length ? listId : undefined}
        autoCapitalize="off"
        autoCorrect="off"
        spellCheck={false}
      />
      {suggestions.length > 0 && (
        <datalist id={listId}>
          {suggestions
            .filter((s) => !value.includes(s))
            .map((s) => (
              <option key={s} value={s} />
            ))}
        </datalist>
      )}
    </div>
  );
}

export function Spinner({ className }: { className?: string }) {
  return (
    <span
      role="status"
      aria-label="Loading"
      className={cn(
        'border-surface-4 border-t-accent inline-block size-5 animate-spin rounded-full border-2',
        className,
      )}
    />
  );
}
