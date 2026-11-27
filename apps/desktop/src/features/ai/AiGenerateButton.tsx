// SPDX-License-Identifier: GPL-3.0-or-later
import { Wand2 } from 'lucide-react';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';

import { Button } from '@/components/ui/button';
import { commands, errorMessage } from '@/lib/ipc';

/** "Magic wand" button that turns a description into a snippet with the local model. */
export function AiGenerateButton({
  description,
  onResult,
}: {
  mode: 'snippet';
  description: string;
  onResult: (script: string) => void;
}) {
  const { t } = useTranslation();
  const [busy, setBusy] = useState(false);
  return (
    <Button
      size="icon"
      variant="ghost"
      aria-label={t('ai.generateSnippet')}
      title={t('ai.generateSnippet')}
      disabled={!description.trim()}
      loading={busy}
      onClick={() => {
        setBusy(true);
        commands
          .aiGenerateSnippet(description)
          .then(onResult)
          .catch((e: unknown) => toast.error(errorMessage(e)))
          .finally(() => setBusy(false));
      }}
    >
      {busy ? null : <Wand2 className="size-4" />}
    </Button>
  );
}
