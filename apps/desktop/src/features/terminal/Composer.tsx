// SPDX-License-Identifier: GPL-3.0-or-later
/**
 * The command editor (Mod+Shift+E): write or edit a long or multi-line command in a real text
 * field, then run it or put it on the prompt. Multi-line text is sent as a bracketed paste so
 * the shell does not run each line on its own.
 */
import { SquarePen } from 'lucide-react';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';

import { Button } from '@/components/ui/button';
import { Kbd } from '@/components/ui/controls';
import { formatShortcut, usePlatform } from '@/lib/platform';

export function Composer({
  initial,
  fontFamily,
  onInsert,
  onRun,
  onClose,
}: {
  initial: string;
  fontFamily: string;
  onInsert: (text: string) => void;
  onRun: (text: string) => void;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const platform = usePlatform();
  const [text, setText] = useState(initial);
  const lines = Math.min(14, Math.max(3, text.split('\n').length + 1));
  return (
    <div
      role="dialog"
      aria-label={t('terminal.composer.title')}
      data-testid="composer"
      className="bg-surface-2/95 border-border animate-fade-in absolute inset-x-3 bottom-3 z-30 overflow-hidden rounded-xl border shadow-[var(--shadow)] backdrop-blur-md"
    >
      <div className="text-muted flex items-center gap-2 px-3 pt-2 text-[12px] font-semibold">
        <SquarePen className="size-3.5" aria-hidden />
        {t('terminal.composer.title')}
      </div>
      <textarea
        autoFocus
        aria-label={t('terminal.composer.title')}
        value={text}
        rows={lines}
        spellCheck={false}
        autoCapitalize="off"
        autoCorrect="off"
        onChange={(e) => setText(e.target.value)}
        onKeyDown={(e) => {
          const mod = e.metaKey || e.ctrlKey;
          if (e.key === 'Escape') {
            e.preventDefault();
            onClose();
          } else if (e.key === 'Enter' && mod && e.shiftKey) {
            e.preventDefault();
            onInsert(text);
          } else if (e.key === 'Enter' && mod) {
            e.preventDefault();
            if (text.trim()) onRun(text);
          }
        }}
        style={{ fontFamily }}
        className="text-text block w-full resize-none bg-transparent px-3 py-2 text-[13px] outline-none"
      />
      <div className="border-border flex flex-wrap items-center gap-2 border-t px-3 py-2">
        <span className="text-subtle mr-auto text-[11px]">{t('terminal.composer.hint')}</span>
        <Button size="sm" variant="ghost" onClick={onClose}>
          {t('common.cancel')}
        </Button>
        <Button size="sm" variant="ghost" onClick={() => onInsert(text)} disabled={!text.trim()}>
          {t('terminal.insert')} <Kbd>{formatShortcut(platform, 'Mod+Shift+↵')}</Kbd>
        </Button>
        <Button size="sm" variant="primary" onClick={() => onRun(text)} disabled={!text.trim()}>
          {t('terminal.run')} <Kbd>{formatShortcut(platform, 'Mod+↵')}</Kbd>
        </Button>
      </div>
    </div>
  );
}
