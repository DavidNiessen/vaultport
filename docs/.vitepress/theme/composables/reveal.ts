// SPDX-License-Identifier: GPL-3.0-or-later
/**
 * `v-reveal` fades an element in the first time it scrolls into view.
 *
 * Elements are only hidden once JavaScript runs, never for people who prefer reduced motion,
 * and never if they are already on screen, so server-rendered content is always readable.
 * The optional binding value is a delay in milliseconds for staggered groups.
 */
import type { Directive } from 'vue';

let observer: IntersectionObserver | null = null;

function getObserver(): IntersectionObserver {
  observer ??= new IntersectionObserver(
    (entries) => {
      for (const entry of entries) {
        if (!entry.isIntersecting) continue;
        entry.target.classList.add('is-visible');
        observer?.unobserve(entry.target);
      }
    },
    { rootMargin: '0px 0px -8% 0px', threshold: 0.12 },
  );
  return observer;
}

function prefersReducedMotion(): boolean {
  return window.matchMedia('(prefers-reduced-motion: reduce)').matches;
}

export const vReveal: Directive<HTMLElement, number | undefined> = {
  mounted(el, binding) {
    if (typeof IntersectionObserver === 'undefined' || prefersReducedMotion()) return;
    const rect = el.getBoundingClientRect();
    if (rect.top < window.innerHeight * 0.92 && rect.bottom > 0) return;
    if (binding.value) el.style.setProperty('--sc-reveal-delay', `${binding.value}ms`);
    el.classList.add('sc-reveal');
    getObserver().observe(el);
  },
  unmounted(el) {
    observer?.unobserve(el);
  },
};
