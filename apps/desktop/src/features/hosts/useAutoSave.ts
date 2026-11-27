// SPDX-License-Identifier: GPL-3.0-or-later
import { useCallback, useEffect, useRef, useState } from 'react';

/**
 * Local editable copy of an entity that is persisted automatically (desktop) after a short
 * debounce, or explicitly via `flush()` (mobile "Save" button). External updates replace the
 * draft only when there are no pending local edits.
 */
export function useAutoSave<T extends { id: string }>(
  source: T,
  save: (value: T) => void,
  { auto, delay = 500 }: { auto: boolean; delay?: number },
) {
  const [draft, setDraft] = useState<T>(source);
  const [dirty, setDirty] = useState(false);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const latest = useRef(draft);
  const saveRef = useRef(save);
  useEffect(() => {
    latest.current = draft;
    saveRef.current = save;
  });

  useEffect(() => {
    if (!dirty || source.id !== latest.current.id) {
      setDraft(source);
      setDirty(false);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps -- only react to new source data
  }, [source]);

  const flush = useCallback(() => {
    if (timer.current) clearTimeout(timer.current);
    timer.current = null;
    saveRef.current(latest.current);
    setDirty(false);
  }, []);

  const update = useCallback(
    (fn: (d: T) => T) => {
      setDraft((d) => {
        const next = fn(d);
        latest.current = next;
        return next;
      });
      setDirty(true);
      if (auto) {
        if (timer.current) clearTimeout(timer.current);
        timer.current = setTimeout(flush, delay);
      }
    },
    [auto, delay, flush],
  );

  // Save pending edits when the inspector closes or switches entity.
  useEffect(
    () => () => {
      if (timer.current) {
        clearTimeout(timer.current);
        saveRef.current(latest.current);
      }
    },
    [],
  );

  return { draft, update, dirty, flush, reset: () => setDraft(source) };
}
