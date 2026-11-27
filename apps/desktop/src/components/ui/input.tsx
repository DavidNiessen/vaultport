// SPDX-License-Identifier: GPL-3.0-or-later
import { Eye, EyeOff } from 'lucide-react';
import {
  forwardRef,
  useId,
  useState,
  type InputHTMLAttributes,
  type ReactNode,
  type TextareaHTMLAttributes,
} from 'react';
import { useTranslation } from 'react-i18next';

import { cn } from '@/lib/cn';

const fieldBase =
  'w-full rounded-[12px] border border-border-input bg-surface-3 px-3.5 text-[13.5px] font-medium text-text placeholder:text-subtle transition-[border-color,box-shadow,background-color] duration-150 hover:border-border-strong focus:border-accent focus:bg-surface-2 focus:ring-3 focus:ring-accent-soft focus:outline-none disabled:opacity-50 aria-[invalid=true]:border-danger aria-[invalid=true]:ring-danger-soft pointer-coarse:text-[15px]';

export const Input = forwardRef<HTMLInputElement, InputHTMLAttributes<HTMLInputElement>>(
  function Input({ className, ...props }, ref) {
    return (
      <input
        ref={ref}
        className={cn(fieldBase, 'h-10 pointer-coarse:h-11', className)}
        autoComplete="off"
        autoCorrect="off"
        autoCapitalize="off"
        spellCheck={false}
        {...props}
      />
    );
  },
);

export const Textarea = forwardRef<
  HTMLTextAreaElement,
  TextareaHTMLAttributes<HTMLTextAreaElement>
>(function Textarea({ className, ...props }, ref) {
  return (
    <textarea
      ref={ref}
      className={cn(fieldBase, 'min-h-24 py-2 leading-relaxed', className)}
      spellCheck={false}
      autoCorrect="off"
      autoCapitalize="off"
      {...props}
    />
  );
});

/** Password input with a reveal toggle. Values are never persisted in UI state longer than needed. */
export const PasswordInput = forwardRef<HTMLInputElement, InputHTMLAttributes<HTMLInputElement>>(
  function PasswordInput({ className, ...props }, ref) {
    const [visible, setVisible] = useState(false);
    const { t } = useTranslation();
    return (
      <div className="relative">
        <input
          ref={ref}
          type={visible ? 'text' : 'password'}
          className={cn(fieldBase, 'h-10 pr-10 pointer-coarse:h-11', className)}
          autoComplete="off"
          spellCheck={false}
          {...props}
        />
        <button
          type="button"
          className="text-subtle hover:text-text absolute inset-y-0 right-0 flex w-9 items-center justify-center transition-colors pointer-coarse:w-11"
          onClick={() => setVisible((v) => !v)}
          aria-label={visible ? t('common.hidePassword') : t('common.showPassword')}
          tabIndex={-1}
        >
          {visible ? <EyeOff className="size-4" /> : <Eye className="size-4" />}
        </button>
      </div>
    );
  },
);

interface FieldProps {
  label: ReactNode;
  hint?: ReactNode;
  error?: ReactNode;
  className?: string;
  children: (id: string) => ReactNode;
}

/** Label + control + hint/error, wired up with matching ids for accessibility. */
export function Field({ label, hint, error, className, children }: FieldProps) {
  const id = useId();
  return (
    <div className={cn('flex flex-col gap-1.5', className)}>
      <label htmlFor={id} className="text-text text-[12.5px] font-bold">
        {label}
      </label>
      {children(id)}
      {error ? (
        <p className="text-danger flex items-center gap-1 text-xs" role="alert">
          {error}
        </p>
      ) : hint ? (
        <p className="text-subtle text-xs">{hint}</p>
      ) : null}
    </div>
  );
}
