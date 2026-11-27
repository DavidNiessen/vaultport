// SPDX-License-Identifier: GPL-3.0-or-later
import { Slot } from '@radix-ui/react-slot';
import { Loader2 } from 'lucide-react';
import { forwardRef, type ButtonHTMLAttributes } from 'react';

import { cn } from '@/lib/cn';

type Variant = 'primary' | 'accent' | 'secondary' | 'ghost' | 'danger' | 'outline';
type Size = 'sm' | 'md' | 'lg' | 'icon' | 'icon-sm';

// Pill buttons. `primary` is the inverse of the page (white on dark, ink on light); `accent`
// uses the colour preset and is kept for the one action a screen is about.
const variants: Record<Variant, string> = {
  primary: 'bg-primary text-primary-fg hover:bg-primary-hover shadow-xs',
  accent: 'bg-accent text-accent-fg hover:bg-accent-hover shadow-xs',
  secondary: 'bg-text/[0.07] text-text hover:bg-text/[0.12]',
  ghost: 'bg-transparent text-muted hover:bg-text/[0.07] hover:text-text',
  danger: 'bg-danger text-white hover:brightness-110 shadow-xs',
  outline: 'border border-border-input bg-transparent text-text hover:bg-text/[0.06]',
};

// Pointer-coarse (touch) screens get 44px hit targets.
const sizes: Record<Size, string> = {
  sm: 'h-8 px-3.5 text-[13px] gap-1.5 pointer-coarse:h-10',
  md: 'h-9 px-4 text-[13.5px] gap-2 pointer-coarse:h-11',
  lg: 'h-11 px-6 text-[15px] gap-2',
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
        'inline-flex shrink-0 items-center justify-center rounded-full font-bold whitespace-nowrap select-none [&_svg]:shrink-0',
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
