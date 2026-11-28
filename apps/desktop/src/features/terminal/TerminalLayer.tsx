// SPDX-License-Identifier: GPL-3.0-or-later
/**
 * Every terminal is rendered once, in one stable place: the terminal layer. Tabs, split panes
 * and focus mode only render an empty *slot* where a terminal should appear, and the layer
 * lays the terminal over its slot. Moving a terminal between a tab and a split therefore never
 * unmounts it — its xterm instance, scrollback and session stay alive.
 */
import { useEffect, useLayoutEffect, useRef, useState, useSyncExternalStore } from 'react';

import { useApp, type TerminalTab } from '@/stores/app';

import { TerminalView } from './TerminalView';

interface Slot {
  el: HTMLElement;
  visible: boolean;
}

const slots = new Map<string, Slot>();
const listeners = new Set<() => void>();
let version = 0;

function changed() {
  version++;
  for (const l of listeners) l();
}

function subscribe(l: () => void) {
  listeners.add(l);
  return () => listeners.delete(l);
}

/** Where terminal `id` should appear; `visible` is false while its tab or pane is hidden. */
export function TerminalSlot({
  id,
  visible,
  className,
}: {
  id: string;
  visible: boolean;
  className?: string;
}) {
  const ref = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    slots.set(id, { el, visible });
    changed();
    const ro = new ResizeObserver(changed);
    ro.observe(el);
    return () => {
      ro.disconnect();
      if (slots.get(id)?.el === el) slots.delete(id);
      changed();
    };
  }, [id, visible]);
  return <div ref={ref} className={className} data-terminal-slot={id} />;
}

interface Placement {
  left: number;
  top: number;
  width: number;
  height: number;
  visible: boolean;
}

function same(a: Record<string, Placement>, b: Record<string, Placement>): boolean {
  const ka = Object.keys(a);
  if (ka.length !== Object.keys(b).length) return false;
  return ka.every((k) => {
    const x = a[k];
    const y = b[k];
    return (
      !!x &&
      !!y &&
      x.visible === y.visible &&
      Math.round(x.left) === Math.round(y.left) &&
      Math.round(x.top) === Math.round(y.top) &&
      Math.round(x.width) === Math.round(y.width) &&
      Math.round(x.height) === Math.round(y.height)
    );
  });
}

/** Renders all terminal tabs (and panes) and keeps each over its slot. */
export function TerminalLayer() {
  const tabs = useApp((s) => s.tabs);
  const terminals = tabs.filter((t): t is TerminalTab => t.kind === 'terminal');
  const layerRef = useRef<HTMLDivElement>(null);
  const [placements, setPlacements] = useState<Record<string, Placement>>({});
  const slotVersion = useSyncExternalStore(subscribe, () => version);

  useLayoutEffect(() => {
    let frame = 0;
    const measure = () => {
      frame = 0;
      const layer = layerRef.current;
      if (!layer) return;
      const base = layer.getBoundingClientRect();
      const next: Record<string, Placement> = {};
      for (const [id, slot] of slots) {
        const r = slot.el.getBoundingClientRect();
        next[id] = {
          left: r.left - base.left,
          top: r.top - base.top,
          width: r.width,
          height: r.height,
          visible: slot.visible,
        };
      }
      setPlacements((cur) => (same(cur, next) ? cur : next));
    };
    measure();
    // Slots can also move without resizing (e.g. a sibling appears); check again next frame.
    frame = requestAnimationFrame(measure);
    return () => cancelAnimationFrame(frame);
  }, [slotVersion, tabs]);

  useEffect(() => {
    window.addEventListener('resize', changed);
    return () => window.removeEventListener('resize', changed);
  }, []);

  return (
    <div ref={layerRef} className="pointer-events-none absolute inset-0" data-terminal-layer>
      {terminals.map((tab) => {
        const p = placements[tab.id];
        const visible = p?.visible ?? false;
        return (
          <div
            key={tab.id}
            className="absolute flex min-h-0 flex-col"
            aria-hidden={!visible}
            style={{
              left: p?.left ?? 0,
              top: p?.top ?? 0,
              // Hidden terminals keep their last size so they do not reflow.
              width: p?.width ?? '100%',
              height: p?.height ?? '100%',
              visibility: visible ? 'visible' : 'hidden',
              pointerEvents: visible ? 'auto' : 'none',
            }}
          >
            <TerminalView tab={tab} active={visible} />
          </div>
        );
      })}
    </div>
  );
}
