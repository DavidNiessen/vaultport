<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script setup lang="ts">
/**
 * A line that starts as ciphertext and is decrypted once by a small key gliding along it: each
 * letter turns readable as the key passes, then the key turns in the lock and fades. Pure CSS
 * once rendered, so it plays exactly once, and with reduced motion the text is simply there.
 * Screen readers and search engines get the plain text.
 */
import { KeyRound } from '@lucide/vue';
import { computed } from 'vue';

const props = withDefaults(defineProps<{ text: string; delay?: number; duration?: number }>(), {
  delay: 600,
  duration: 1300,
});

/** Deterministic base64-ish ciphertext, so server and client render the same glyphs. */
const chars = computed(() => {
  const A = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/';
  const list = [...props.text];
  let h = 2166136261;
  return list.map((c, i) => {
    h = Math.imul(h ^ (c.charCodeAt(0) + i), 16777619) >>> 0;
    return {
      c,
      glyph: c === ' ' ? ' ' : A[h % 64],
      // When the key reaches this letter, and where along the gradient it sits.
      at: props.delay + (props.duration * (i + 0.5)) / list.length,
      mix: Math.round((i / Math.max(1, list.length - 1)) * 100),
    };
  });
});
</script>

<template>
  <span class="key-reveal" :style="{ '--delay': `${delay}ms`, '--dur': `${duration}ms` }">
    <span class="sr-only">{{ text }}</span>
    <span class="glyphs" aria-hidden="true">
      <span
        v-for="(ch, i) in chars"
        :key="i"
        class="ch"
        :style="{ '--at': `${ch.at}ms`, '--mix': `${ch.mix}%` }"
        ><span class="plain">{{ ch.c === ' ' ? ' ' : ch.c }}</span
        ><span v-if="ch.c !== ' '" class="cipher">{{ ch.glyph }}</span></span
      >
      <span class="key"><KeyRound class="key-icon" /></span>
    </span>
  </span>
</template>

<style scoped>
.key-reveal {
  position: relative;
  display: inline-block;
}
.glyphs {
  position: relative;
  display: inline-block;
}
.ch {
  position: relative;
  display: inline-block;
}
/* The letters carry the gradient themselves, so animating them never fights background-clip. */
.plain {
  color: color-mix(in oklab, rgb(var(--glow-1)) calc(100% - var(--mix)), rgb(var(--glow-2)));
  animation: plain-in 0.55s var(--ease-out) both;
  animation-delay: var(--at);
}
.cipher {
  position: absolute;
  inset: 0;
  display: grid;
  place-items: center;
  font-family: var(--font-mono);
  font-style: normal;
  font-size: 0.42em;
  color: var(--text-3);
  opacity: 0;
  animation: cipher-out 0.4s ease-out both;
  animation-delay: var(--at);
}
@keyframes plain-in {
  from {
    opacity: 0;
    filter: blur(6px);
  }
}
@keyframes cipher-out {
  from {
    opacity: 0.9;
  }
  to {
    opacity: 0;
  }
}

.key {
  position: absolute;
  top: 50%;
  left: 0;
  width: 0.34em;
  height: 0.34em;
  margin: -0.17em 0 0 -0.17em;
  color: var(--text);
  filter: drop-shadow(0 0 0.12em rgb(var(--glow-1) / 0.9));
  opacity: 0;
  pointer-events: none;
  animation:
    key-travel var(--dur) cubic-bezier(0.45, 0, 0.3, 1) var(--delay) both,
    key-turn 0.7s var(--ease-out) calc(var(--delay) + var(--dur)) forwards;
}
.key-icon {
  display: block;
  width: 100%;
  height: 100%;
  stroke-width: 2.25;
}
@keyframes key-travel {
  0% {
    left: 0;
    opacity: 0;
  }
  12% {
    opacity: 1;
  }
  100% {
    left: 100%;
    opacity: 1;
  }
}
@keyframes key-turn {
  from {
    left: 100%;
    opacity: 1;
    transform: translateX(0.18em) rotate(0deg);
  }
  to {
    left: 100%;
    opacity: 0;
    transform: translateX(0.18em) rotate(90deg);
  }
}

@media (prefers-reduced-motion: reduce) {
  .plain,
  .cipher,
  .key {
    animation: none;
  }
  .key,
  .cipher {
    display: none;
  }
}
</style>
