// SPDX-License-Identifier: GPL-3.0-or-later
import { Slot } from '@radix-ui/react-slot';
import { Loader2 } from 'lucide-react';
import { forwardRef, type ButtonHTMLAttributes } from 'react';

import { cn } from '@/lib/cn';

type Variant = 'primary' | 'secondary' | 'ghost' | 'danger' | 'outline';
type Size = 'sm' | 'md' | 'lg' | 'icon' | 'icon-sm';

const variants: Record<Variant, string> = {
  primary:
    'bg-accent text-accent-fg hover:bg-accent-hover shadow-xs shadow-[inset_0_1px_0_rgb(255_255_255/0.14)]',
  secondary: 'bg-surface-3 text-text border border-border hover:bg-surface-4',
  ghost: 'bg-transparent text-muted hover:bg-surface-3 hover:text-text',
  danger: 'bg-danger text-white hover:brightness-110 shadow-xs',
  outline: 'border border-border-input bg-transparent text-text hover:bg-surface-3',
};

// Pointer-coarse (touch) screens get 44px hit targets.
const sizes: Record<Size, string> = {
  sm: 'h-8 px-3 text-[13px] gap-1.5 pointer-coarse:h-10',
  md: 'h-9 px-3.5 text-[13px] gap-2 pointer-coarse:h-11',
  lg: 'h-11 px-5 text-[14px] gap-2',
  icon: 'size-9 pointer-coarse:size-11',
  'icon-sm': 'size-7 pointer-coarse:size-10',
};

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: Variant;
  size?: Size;
  loading?: boolean;
  asChild?: boolean;
}

export const Button = forwardRef<HTMLButtonElement, ButtonProps>(function Button(
  {
    className,
    variant = 'secondary',
    size = 'md',
    loading = false,
    asChild = false,
    disabled,
    children,
    type,
    ...props
  },
  ref,
) {
  const Comp = asChild ? Slot : 'button';
  return (
    <Comp
      ref={ref}
      type={asChild ? undefined : (type ?? 'button')}
      disabled={disabled ?? loading}
      className={cn(
        'inline-flex shrink-0 items-center justify-center rounded-[var(--radius-control)] font-medium whitespace-nowrap select-none [&_svg]:shrink-0',
        'transition-[background-color,color,border-color,box-shadow,transform] duration-150 active:translate-y-px',
        'disabled:pointer-events-none disabled:opacity-45',
        variants[variant],
        sizes[size],
        className,
      )}
      {...props}
    >
      {loading ? <Loader2 className="size-4 animate-spin" aria-hidden /> : null}
      {children}
    </Comp>
  );
});
