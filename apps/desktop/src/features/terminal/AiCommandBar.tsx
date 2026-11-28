// SPDX-License-Identifier: GPL-3.0-or-later
/**
 * The AI command bar (Ctrl+` or typing `# what you want` at the prompt): describe a task in
 * your own words and the on-device model suggests one command, with an explanation and a risk
 * rating. It is only ever put on the prompt or run when you say so. In "fix" mode it suggests
 * a correction for the last command that failed.
 */
import { AlertTriangle, Loader2, Settings, Sparkles, Wand2 } from 'lucide-react';
import { useCallback, useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { Button } from '@/components/ui/button';
import { Kbd } from '@/components/ui/controls';
import { cn } from '@/lib/cn';
import { commands, errorMessage, type AiSuggestion } from '@/lib/ipc';
import { formatShortcut, usePlatform } from '@/lib/platform';

export type AiRequest = { mode: 'ask'; prompt: string } | { mode: 'fix' };

const RISK_STYLE: Record<string, string> = {
  low: 'bg-success-soft text-success',
  medium: 'bg-warning-soft text-warning',
  high: 'bg-danger-soft text-danger',
  critical: 'bg-danger text-white',
};

export function AiCommandBar({
  request,
  enabled,
  sessionId,
  selection,
  onInsert,
  onRun,
  onClose,
  onOpenSettings,
}: {
  request: AiRequest;
  enabled: boolean;
  sessionId: string | null;
  selection: string;
  onInsert: (command: string) => void;
  onRun: (command: string) => void;
  onClose: () => void;
  onOpenSettings: () => void;
}) {
  const { t } = useTranslation();
  const platform = usePlatform();
  const [prompt, setPrompt] = useState(request.mode === 'ask' ? request.prompt : '');
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<AiSuggestion | null>(null);
  const [error, setError] = useState<string | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const started = useRef(false);

  const generate = useCallback(
    async (text: string, fix: boolean) => {
      setBusy(true);
      setError(null);
      setResult(null);
      try {
        const s = fix
          ? await commands.aiFixCommand(sessionId ?? '')
          : await commands.aiGenerateCommand(text, sessionId, selection || null);
        setResult(s);
      } catch (e) {
        setError(errorMessage(e));
      } finally {
        setBusy(false);
      }
    },
    [sessionId, selection],
  );

  // A request that arrives with a prompt (from `# …`) or in fix mode starts right away.
  useEffect(() => {
    if (started.current || !enabled) return;
    // Started after the effect, so the first render does not set state synchronously.
    if (request.mode === 'fix') {
      started.current = true;
      void Promise.resolve().then(() => generate('', true));
    } else if (request.prompt.trim()) {
      started.current = true;
      const text = request.prompt;
      void Promise.resolve().then(() => generate(text, false));
    }
  }, [enabled, generate, request]);

  const keys = (e: React.KeyboardEvent) => {
    const mod = e.metaKey || e.ctrlKey;
    if (e.key === 'Escape') {
      e.preventDefault();
      onClose();
    } else if (e.key === 'Enter' && result && (mod || e.shiftKey)) {
      e.preventDefault();
      onRun(result.command);
    } else if (e.key === 'Tab' && result) {
      e.preventDefault();
      onInsert(result.command);
    } else if (e.key === 'Enter' && !busy) {
      e.preventDefault();
      if (result && prompt.trim() === '') onInsert(result.command);
      else if (prompt.trim()) void generate(prompt, false);
    }
  };

  return (
    <div
      role="dialog"
      aria-label={t('terminal.ai.title')}
      data-testid="ai-bar"
      className="bg-surface-2/95 border-border animate-fade-in absolute inset-x-3 bottom-3 z-30 overflow-hidden rounded-xl border shadow-[var(--shadow)] backdrop-blur-md"
    >
      {!enabled ? (
        <div className="flex flex-wrap items-center gap-3 p-3 text-[13px]">
          <Sparkles className="text-accent-text size-4" aria-hidden />
          <span className="text-muted min-w-0 flex-1">{t('terminal.ai.disabled')}</span>
          <Button size="sm" variant="ghost" onClick={onClose}>
            {t('common.close')}
          </Button>
          <Button size="sm" variant="primary" onClick={onOpenSettings}>
            <Settings className="size-4" />
            {t('terminal.ai.openSettings')}
          </Button>
        </div>
      ) : (
        <>
          <div className="flex items-center gap-2 px-3">
            {request.mode === 'fix' ? (
              <Wand2 className="text-accent-text size-4 shrink-0" aria-hidden />
            ) : (
              <Sparkles className="text-accent-text size-4 shrink-0" aria-hidden />
            )}
            {request.mode === 'fix' ? (
              <span className="text-muted flex h-11 flex-1 items-center text-[13px]">
                {t('terminal.ai.fixing')}
              </span>
            ) : (
              <input
                ref={inputRef}
                autoFocus
                aria-label={t('terminal.ai.placeholder')}
                placeholder={t('terminal.ai.placeholder')}
                value={prompt}
                onChange={(e) => setPrompt(e.target.value)}
                onKeyDown={keys}
                className="text-text placeholder:text-subtle h-11 min-w-0 flex-1 bg-transparent text-[13px] outline-none"
              />
            )}
            {busy ? <Loader2 className="text-subtle size-4 animate-spin" aria-hidden /> : null}
          </div>
          {error ? (
            <div className="border-border text-danger flex items-start gap-2 border-t px-3 py-2 text-[13px]">
              <AlertTriangle className="mt-0.5 size-4 shrink-0" aria-hidden />
              <span className="min-w-0 flex-1">{error}</span>
              <Button
                size="sm"
                variant="ghost"
                onClick={onClose}
                autoFocus={request.mode === 'fix'}
              >
                {t('common.close')}
              </Button>
            </div>
          ) : null}
          {result ? (
            <div
              className="border-border grid gap-2 border-t px-3 py-2.5"
              // In fix mode there is no input, so the result takes the keys.
              tabIndex={request.mode === 'fix' ? 0 : -1}
              onKeyDown={request.mode === 'fix' ? keys : undefined}
              ref={(el) => {
                if (el && request.mode === 'fix') el.focus();
              }}
            >
              <div className="flex items-start gap-2">
                <code className="bg-surface-3 text-text min-w-0 flex-1 rounded-lg px-2.5 py-1.5 font-mono text-[13px] break-all">
                  {result.command}
                </code>
                <span
                  className={cn(
                    'shrink-0 rounded-full px-2 py-0.5 text-[11px] font-semibold',
                    RISK_STYLE[result.risk] ?? RISK_STYLE.medium,
                  )}
                >
                  {t(`terminal.ai.risk.${result.risk}`, { defaultValue: result.risk })}
                </span>
              </div>
              {result.explanation ? (
                <p className="text-muted text-[12px]">{result.explanation}</p>
              ) : null}
              <div className="flex flex-wrap items-center gap-2">
                <Button size="sm" variant="ghost" onClick={() => onInsert(result.command)}>
                  {t('terminal.insert')} <Kbd>Tab</Kbd>
                </Button>
                <Button size="sm" variant="primary" onClick={() => onRun(result.command)}>
                  {t('terminal.run')} <Kbd>{formatShortcut(platform, 'Mod+↵')}</Kbd>
                </Button>
                <span className="text-subtle ml-auto text-[11px]">{t('terminal.ai.local')}</span>
              </div>
            </div>
          ) : null}
        </>
      )}
    </div>
  );
}
