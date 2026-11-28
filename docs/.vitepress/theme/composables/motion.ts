// SPDX-License-Identifier: GPL-3.0-or-later
/**
 * Motion helpers: scroll reveal, cursor spotlight, magnetic and tilt directives, and
 * scroll-linked progress. Everything respects `prefers-reduced-motion` and does nothing during
 * server rendering.
 */
import { onBeforeUnmount, onMounted, ref, type Directive, type Ref } from 'vue';

export function prefersReducedMotion(): boolean {
  return (
    typeof window !== 'undefined' && window.matchMedia('(prefers-reduced-motion: reduce)').matches
  );
}

/** Reactive `prefers-reduced-motion`, `false` until mounted. */
export function useReducedMotion(): Ref<boolean> {
  const reduced = ref(false);
  let mq: MediaQueryList | null = null;
  const update = () => (reduced.value = Boolean(mq?.matches));
  onMounted(() => {
    mq = window.matchMedia('(prefers-reduced-motion: reduce)');
    update();
    mq.addEventListener('change', update);
  });
  onBeforeUnmount(() => mq?.removeEventListener('change', update));
  return reduced;
}

const finePointer = () =>
  typeof window !== 'undefined' && window.matchMedia('(hover: hover) and (pointer: fine)').matches;

/* ---------------------------------------------------------------- reveal */

type RevealValue = number | { delay?: number } | undefined;

let revealObserver: IntersectionObserver | null = null;
function getRevealObserver(): IntersectionObserver {
  revealObserver ??= new IntersectionObserver(
    (entries) => {
      for (const entry of entries) {
        if (!entry.isIntersecting) continue;
        (entry.target as HTMLElement).dataset.reveal = 'in';
        revealObserver?.unobserve(entry.target);
      }
    },
    { rootMargin: '0px 0px -8% 0px', threshold: 0.08 },
  );
  return revealObserver;
}

/** `v-reveal` or `v-reveal="120"` (delay in ms): fades, unblurs and lifts the element in once. */
export const vReveal: Directive<HTMLElement, RevealValue> = {
  getSSRProps(binding) {
    const delay = typeof binding.value === 'number' ? binding.value : binding.value?.delay;
    return {
      'data-reveal': '',
      ...(delay ? { style: { '--reveal-delay': `${delay}ms` } } : {}),
    };
  },
  mounted(el, binding) {
    // A data attribute rather than a class: Vue rewrites `class` whenever a `:class` binding on
    // the same element changes, which would hide an element that had already been revealed.
    if (el.dataset.reveal !== 'in') el.dataset.reveal = '';
    const delay = typeof binding.value === 'number' ? binding.value : binding.value?.delay;
    if (delay) el.style.setProperty('--reveal-delay', `${delay}ms`);
    if (prefersReducedMotion() || !('IntersectionObserver' in window)) {
      el.dataset.reveal = 'in';
      return;
    }
    getRevealObserver().observe(el);
  },
  beforeUnmount(el) {
    revealObserver?.unobserve(el);
  },
};

/* ------------------------------------------------------------- spotlight */

type Cleanup = () => void;
const cleanups = new WeakMap<HTMLElement, Cleanup>();

/** `v-spotlight`: exposes the pointer position as `--mx`/`--my` for the `.spotlight` glow. */
export const vSpotlight: Directive<HTMLElement> = {
  getSSRProps: () => ({}),
  mounted(el) {
    el.classList.add('spotlight');
    const move = (e: PointerEvent) => {
      const r = el.getBoundingClientRect();
      el.style.setProperty('--mx', `${e.clientX - r.left}px`);
      el.style.setProperty('--my', `${e.clientY - r.top}px`);
    };
    el.addEventListener('pointermove', move);
    cleanups.set(el, () => el.removeEventListener('pointermove', move));
  },
  beforeUnmount(el) {
    cleanups.get(el)?.();
  },
};

/* -------------------------------------------------------------- magnetic */

/** `v-magnetic` or `v-magnetic="0.35"`: the element leans toward the pointer while hovered. */
export const vMagnetic: Directive<HTMLElement, number | undefined> = {
  getSSRProps: () => ({}),
  mounted(el, binding) {
    if (!finePointer() || prefersReducedMotion()) return;
    const strength = binding.value ?? 0.3;
    el.style.transition = 'transform 0.5s var(--ease-out)';
    const move = (e: PointerEvent) => {
      const r = el.getBoundingClientRect();
      const x = (e.clientX - (r.left + r.width / 2)) * strength;
      const y = (e.clientY - (r.top + r.height / 2)) * strength;
      el.style.transform = `translate3d(${x}px, ${y}px, 0)`;
    };
    const leave = () => (el.style.transform = '');
    el.addEventListener('pointermove', move);
    el.addEventListener('pointerleave', leave);
    cleanups.set(el, () => {
      el.removeEventListener('pointermove', move);
      el.removeEventListener('pointerleave', leave);
    });
  },
  beforeUnmount(el) {
    cleanups.get(el)?.();
  },
};

/* ------------------------------------------------------------------ tilt */

/** `v-tilt` or `v-tilt="8"` (max degrees): a 3D tilt toward the pointer, with `--glare-x/y`. */
export const vTilt: Directive<HTMLElement, number | undefined> = {
  getSSRProps: () => ({}),
  mounted(el, binding) {
    if (!finePointer() || prefersReducedMotion()) return;
    const max = binding.value ?? 6;
    let frame = 0;
    const move = (e: PointerEvent) => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(() => {
        const r = el.getBoundingClientRect();
        const px = (e.clientX - r.left) / r.width;
        const py = (e.clientY - r.top) / r.height;
        el.style.transition = 'transform 0.15s linear';
        el.style.transform = `perspective(1000px) rotateX(${(0.5 - py) * max}deg) rotateY(${(px - 0.5) * max}deg)`;
        el.style.setProperty('--glare-x', `${px * 100}%`);
        el.style.setProperty('--glare-y', `${py * 100}%`);
      });
    };
    const leave = () => {
      cancelAnimationFrame(frame);
      el.style.transition = 'transform 0.8s var(--ease-out)';
      el.style.transform = '';
    };
    el.addEventListener('pointermove', move);
    el.addEventListener('pointerleave', leave);
    cleanups.set(el, () => {
      el.removeEventListener('pointermove', move);
      el.removeEventListener('pointerleave', leave);
    });
  },
  beforeUnmount(el) {
    cleanups.get(el)?.();
  },
};

/* ------------------------------------------------------- scroll progress */

/**
 * Progress of an element through the viewport: 0 when its top meets the bottom of the viewport,
 * 1 when its bottom meets the top (or, with `mode: 'pin'`, across its own scrollable height —
 * for sticky sections taller than the viewport).
 */
export function useScrollProgress(
  target: Ref<HTMLElement | null | undefined>,
  mode: 'through' | 'pin' = 'through',
): Ref<number> {
  const progress = ref(0);
  let frame = 0;
  const update = () => {
    frame = 0;
    const el = target.value;
    if (!el) return;
    const r = el.getBoundingClientRect();
    const vh = window.innerHeight;
    const p =
      mode === 'pin'
        ? -r.top / Math.max(1, r.height - vh)
        : (vh - r.top) / Math.max(1, r.height + vh);
    progress.value = Math.min(1, Math.max(0, p));
  };
  const onScroll = () => {
    if (!frame) frame = requestAnimationFrame(update);
  };
  onMounted(() => {
    update();
    window.addEventListener('scroll', onScroll, { passive: true });
    window.addEventListener('resize', onScroll, { passive: true });
  });
  onBeforeUnmount(() => {
    cancelAnimationFrame(frame);
    window.removeEventListener('scroll', onScroll);
    window.removeEventListener('resize', onScroll);
  });
  return progress;
}

/** True while the element is (nearly) on screen, to pause animation loops off screen. */
export function useInView(
  target: Ref<HTMLElement | null | undefined>,
  margin = '100px',
): Ref<boolean> {
  const visible = ref(false);
  let observer: IntersectionObserver | null = null;
  onMounted(() => {
    if (!target.value) return;
    observer = new IntersectionObserver(
      ([entry]) => {
        visible.value = Boolean(entry?.isIntersecting);
      },
      { rootMargin: margin },
    );
    observer.observe(target.value);
  });
  onBeforeUnmount(() => observer?.disconnect());
  return visible;
}
