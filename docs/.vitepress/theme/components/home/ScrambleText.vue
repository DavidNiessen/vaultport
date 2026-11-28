<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script setup lang="ts">
/**
 * Text that "decrypts": each character cycles through cipher glyphs before settling, left to
 * right. Starts when it scrolls into view (or after `delay`); can replay on hover. The real text
 * is always what assistive technology and search engines read.
 */
import { onBeforeUnmount, onMounted, ref } from 'vue';

import { prefersReducedMotion } from '../../composables/motion';

const props = withDefaults(
  defineProps<{ text: string; delay?: number; duration?: number; hover?: boolean }>(),
  { delay: 0, duration: 1100, hover: false },
);

const GLYPHS = '0123456789abcdef#%&*+=<>/\\[]{}$@';
const shown = ref(props.text);
const el = ref<HTMLElement | null>(null);
let raf = 0;
let timer = 0;
let observer: IntersectionObserver | null = null;

function run() {
  cancelAnimationFrame(raf);
  if (prefersReducedMotion()) {
    shown.value = props.text;
    return;
  }
  const chars = [...props.text];
  const start = performance.now();
  const step = (now: number) => {
    const t = (now - start) / props.duration;
    shown.value = chars
      .map((c, i) => {
        if (c === ' ') return ' ';
        const settle = (i / chars.length) * 0.75 + 0.25;
        return t >= settle ? c : GLYPHS[Math.floor(Math.random() * GLYPHS.length)];
      })
      .join('');
    if (t < 1) raf = requestAnimationFrame(step);
    else shown.value = props.text;
  };
  raf = requestAnimationFrame(step);
}

onMounted(() => {
  if (!el.value) return;
  observer = new IntersectionObserver(([entry]) => {
    if (!entry?.isIntersecting) return;
    observer?.disconnect();
    timer = window.setTimeout(run, props.delay);
  });
  observer.observe(el.value);
});
onBeforeUnmount(() => {
  cancelAnimationFrame(raf);
  clearTimeout(timer);
  observer?.disconnect();
});
</script>

<template>
  <span ref="el" class="scramble" @pointerenter="hover && run()">
    <span class="sr-only">{{ text }}</span>
    <span aria-hidden="true">{{ shown }}</span>
  </span>
</template>

<style scoped>
.scramble {
  white-space: pre-wrap;
}
</style>
