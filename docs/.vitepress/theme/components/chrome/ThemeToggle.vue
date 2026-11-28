<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script setup lang="ts">
/**
 * Light/dark switch. Where the View Transitions API exists, the new theme grows as a circle out
 * of the button; the icon morphs between a sun and a crescent.
 */
import { useData } from 'vitepress';
import { nextTick, ref } from 'vue';

import { prefersReducedMotion } from '../../composables/motion';

const { isDark } = useData();
const button = ref<HTMLButtonElement | null>(null);

type ViewTransitionDocument = Document & {
  startViewTransition?: (cb: () => Promise<void>) => { ready: Promise<void> };
};

async function toggle(e: MouseEvent) {
  const doc = document as ViewTransitionDocument;
  if (!doc.startViewTransition || prefersReducedMotion()) {
    isDark.value = !isDark.value;
    return;
  }
  const rect = button.value?.getBoundingClientRect();
  const x = e.clientX || (rect ? rect.left + rect.width / 2 : innerWidth / 2);
  const y = e.clientY || (rect ? rect.top + rect.height / 2 : 0);
  const radius = Math.hypot(Math.max(x, innerWidth - x), Math.max(y, innerHeight - y));
  const transition = doc.startViewTransition(async () => {
    isDark.value = !isDark.value;
    await nextTick();
  });
  await transition.ready;
  document.documentElement.animate(
    { clipPath: [`circle(0px at ${x}px ${y}px)`, `circle(${radius}px at ${x}px ${y}px)`] },
    {
      duration: 650,
      easing: 'cubic-bezier(0.65, 0, 0.35, 1)',
      pseudoElement: '::view-transition-new(root)',
    },
  );
}
</script>

<template>
  <button
    ref="button"
    type="button"
    class="theme-toggle"
    aria-label="Toggle dark theme"
    title="Toggle dark theme"
    @click="toggle"
  >
    <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true">
      <mask id="tt-moon">
        <rect width="24" height="24" fill="#fff" />
        <circle class="bite" cx="24" cy="4" r="7" fill="#000" />
      </mask>
      <circle class="core" cx="12" cy="12" r="5" mask="url(#tt-moon)" fill="currentColor" />
      <g class="rays" stroke="currentColor" stroke-width="2" stroke-linecap="round">
        <path
          d="M12 1.5v2M12 20.5v2M4.6 4.6l1.4 1.4M18 18l1.4 1.4M1.5 12h2M20.5 12h2M4.6 19.4 6 18M18 6l1.4-1.4"
        />
      </g>
    </svg>
  </button>
</template>

<style scoped>
.theme-toggle {
  display: grid;
  place-items: center;
  width: 38px;
  height: 38px;
  border-radius: 999px;
  color: var(--text-2);
  transition:
    color 0.2s,
    background-color 0.2s;
}
.theme-toggle:hover {
  color: var(--text);
  background: var(--surface-2);
}
svg {
  overflow: visible;
}
.core {
  transition: r 0.5s var(--ease-out);
}
.bite {
  transition:
    cx 0.5s var(--ease-out),
    cy 0.5s var(--ease-out);
}
.rays {
  transform-origin: 12px 12px;
  transition:
    transform 0.6s var(--ease-spring),
    opacity 0.3s;
}
</style>

<!-- Not scoped: the icon follows html.dark (a class binding would keep the server's guess). -->
<style>
html.dark .theme-toggle .core {
  r: 8.5px;
}
html.dark .theme-toggle .bite {
  cx: 16px;
  cy: 7px;
}
html.dark .theme-toggle .rays {
  transform: rotate(-90deg) scale(0.4);
  opacity: 0;
}
</style>
