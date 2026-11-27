// SPDX-License-Identifier: GPL-3.0-or-later
import * as DialogPrimitive from '@radix-ui/react-dialog';
import * as DropdownPrimitive from '@radix-ui/react-dropdown-menu';
import * as ContextPrimitive from '@radix-ui/react-context-menu';
import { X } from 'lucide-react';
import type { ReactNode } from 'react';
import { useTranslation } from 'react-i18next';

import { cn } from '@/lib/cn';

interface DialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title: ReactNode;
  description?: ReactNode;
  children: ReactNode;
  footer?: ReactNode;
  className?: string;
}

/** Centered modal dialog (bottom sheet on phones). */
export function Dialog({
  open,
  onOpenChange,
  title,
  description,
  children,
  footer,
  className,
}: DialogProps) {
  const { t } = useTranslation();
  return (
    <DialogPrimitive.Root open={open} onOpenChange={onOpenChange}>
      <DialogPrimitive.Portal>
        <DialogPrimitive.Overlay className="bg-overlay animate-fade-in fixed inset-0 z-50 backdrop-blur-[3px]" />
        <DialogPrimitive.Content
          className={cn(
            'bg-surface-1 border-border-strong fixed z-50 flex max-h-[min(88vh,760px)] flex-col border shadow-[var(--shadow)] focus:outline-none',
            'inset-x-0 bottom-0 rounded-t-2xl pb-[env(safe-area-inset-bottom)]',
            'animate-slide-up sm:animate-pop sm:inset-x-auto sm:top-1/2 sm:bottom-auto sm:left-1/2 sm:w-[min(92vw,520px)] sm:-translate-x-1/2 sm:-translate-y-1/2 sm:rounded-[22px] sm:pb-0',
            className,
          )}
        >
          <div className="flex items-start justify-between gap-4 px-5 pt-5 pb-3">
            <div className="min-w-0">
              <DialogPrimitive.Title className="text-[17px] font-extrabold tracking-tight">
                {title}
              </DialogPrimitive.Title>
              {description ? (
                <DialogPrimitive.Description className="text-muted mt-1 text-[13px]">
                  {description}
                </DialogPrimitive.Description>
              ) : (
                <DialogPrimitive.Description className="sr-only">
                  {title}
                </DialogPrimitive.Description>
              )}
            </div>
            <DialogPrimitive.Close
              className="text-subtle hover:bg-text/[0.08] hover:text-text -mt-1 -mr-1 rounded-full p-2 transition-colors pointer-coarse:p-2.5"
              aria-label={t('common.close')}
            >
              <X className="size-4" />
            </DialogPrimitive.Close>
          </div>
          <div className="min-h-0 flex-1 overflow-y-auto px-5 pb-4">{children}</div>
          {footer ? (
            <div className="border-border flex justify-end gap-2 border-t px-5 py-3.5">
              {footer}
            </div>
          ) : null}
        </DialogPrimitive.Content>
      </DialogPrimitive.Portal>
    </DialogPrimitive.Root>
  );
}

const menuContent =
  'bg-surface-3 animate-pop z-50 min-w-52 overflow-hidden rounded-[14px] p-1.5 shadow-[var(--shadow)]';
const menuItem =
  'text-text data-[highlighted]:bg-text/[0.08] flex h-9 cursor-pointer items-center gap-2.5 rounded-[9px] px-2.5 text-[13.5px] font-bold outline-none select-none data-[disabled]:cursor-not-allowed data-[disabled]:opacity-40 pointer-coarse:h-11 [&>svg]:size-4 [&>svg]:text-muted';

export interface MenuEntry {
  label: ReactNode;
  icon?: ReactNode;
  onSelect?: () => void;
  danger?: boolean;
  disabled?: boolean;
  shortcut?: string;
  separator?: false;
}

export type MenuItemDef = MenuEntry | { separator: true };

function renderEntries(
  items: MenuItemDef[],
  Item: typeof DropdownPrimitive.Item | typeof ContextPrimitive.Item,
  Sep: typeof DropdownPrimitive.Separator | typeof ContextPrimitive.Separator,
) {
  return items.map((item, i) =>
    item.separator ? (
      <Sep key={`sep-${String(i)}`} className="bg-border-strong mx-1 my-1.5 h-px" />
    ) : (
      <Item
        key={`item-${String(i)}`}
        className={cn(menuItem, item.danger && 'text-danger [&>svg]:text-danger')}
        disabled={item.disabled}
        onSelect={item.onSelect}
      >
        {item.icon}
        <span className="flex-1">{item.label}</span>
        {item.shortcut ? <span className="text-subtle text-[11px]">{item.shortcut}</span> : null}
      </Item>
    ),
  );
}

export function DropdownMenu({
  trigger,
  items,
  align = 'end',
}: {
  trigger: ReactNode;
  items: MenuItemDef[];
  align?: 'start' | 'end' | 'center';
}) {
  return (
    <DropdownPrimitive.Root>
      <DropdownPrimitive.Trigger asChild>{trigger}</DropdownPrimitive.Trigger>
      <DropdownPrimitive.Portal>
        <DropdownPrimitive.Content align={align} sideOffset={6} className={menuContent}>
          {renderEntries(items, DropdownPrimitive.Item, DropdownPrimitive.Separator)}
        </DropdownPrimitive.Content>
      </DropdownPrimitive.Portal>
    </DropdownPrimitive.Root>
  );
}

export function ContextMenu({ children, items }: { children: ReactNode; items: MenuItemDef[] }) {
  return (
    <ContextPrimitive.Root>
      <ContextPrimitive.Trigger asChild>{children}</ContextPrimitive.Trigger>
      <ContextPrimitive.Portal>
        <ContextPrimitive.Content className={menuContent}>
          {renderEntries(items, ContextPrimitive.Item, ContextPrimitive.Separator)}
        </ContextPrimitive.Content>
      </ContextPrimitive.Portal>
    </ContextPrimitive.Root>
  );
}

/** Slide-in side panel (desktop inspector); becomes a full-height sheet on small screens. */
export function Sheet({
  open,
  onOpenChange,
  title,
  children,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title: ReactNode;
  children: ReactNode;
}) {
  return (
    <DialogPrimitive.Root open={open} onOpenChange={onOpenChange}>
      <DialogPrimitive.Portal>
        <DialogPrimitive.Overlay className="bg-overlay animate-fade-in fixed inset-0 z-40" />
        <DialogPrimitive.Content className="bg-surface-1 border-border animate-slide-up fixed inset-x-0 bottom-0 z-40 flex max-h-[92vh] flex-col rounded-t-2xl border-t pb-[env(safe-area-inset-bottom)] shadow-[var(--shadow)] focus:outline-none">
          <div className="bg-surface-4 mx-auto mt-2 h-1 w-10 rounded-full" aria-hidden />
          <DialogPrimitive.Title className="px-5 pt-3 pb-2 text-[15px] font-semibold">
            {title}
          </DialogPrimitive.Title>
          <DialogPrimitive.Description className="sr-only">{title}</DialogPrimitive.Description>
          <div className="min-h-0 flex-1 overflow-y-auto">{children}</div>
        </DialogPrimitive.Content>
      </DialogPrimitive.Portal>
    </DialogPrimitive.Root>
  );
}
