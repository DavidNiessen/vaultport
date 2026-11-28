<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script setup lang="ts">
/** The app mark, inline so the prompt cursor can blink and the glow can follow the theme. */
withDefaults(defineProps<{ size?: number; animated?: boolean }>(), { size: 30, animated: true });
</script>

<template>
  <svg
    class="logo-mark"
    :class="{ animated }"
    :width="size"
    :height="size"
    viewBox="64 64 896 896"
    aria-hidden="true"
  >
    <defs>
      <linearGradient id="lm-tile" x1="0" y1="0" x2="0" y2="1">
        <stop offset="0" stop-color="#1C1930" />
        <stop offset="1" stop-color="#0D0C12" />
      </linearGradient>
      <radialGradient id="lm-glow" cx="0.3" cy="0.26" r="0.7">
        <stop offset="0" stop-color="#8C78FF" stop-opacity="0.9" />
        <stop offset="0.6" stop-color="#6E5AF0" stop-opacity="0.2" />
        <stop offset="1" stop-color="#6E5AF0" stop-opacity="0" />
      </radialGradient>
      <radialGradient id="lm-glow-b" cx="0.82" cy="0.86" r="0.55">
        <stop offset="0" stop-color="#5A8CE6" stop-opacity="0.7" />
        <stop offset="1" stop-color="#5A8CE6" stop-opacity="0" />
      </radialGradient>
      <linearGradient id="lm-pill" x1="0" y1="0" x2="1" y2="0">
        <stop offset="0" stop-color="#C6BCFF" />
        <stop offset="1" stop-color="#9D8BFF" />
      </linearGradient>
    </defs>
    <rect x="64" y="64" width="896" height="896" rx="232" fill="url(#lm-tile)" />
    <rect x="64" y="64" width="896" height="896" rx="232" fill="url(#lm-glow)" />
    <rect x="64" y="64" width="896" height="896" rx="232" fill="url(#lm-glow-b)" />
    <rect
      x="66"
      y="66"
      width="892"
      height="892"
      rx="230"
      fill="none"
      stroke="#fff"
      stroke-opacity="0.12"
      stroke-width="6"
    />
    <path
      class="chevron"
      d="M286 352 L462 512 L286 672"
      fill="none"
      stroke="#fff"
      stroke-width="112"
      stroke-linecap="round"
      stroke-linejoin="round"
    />
    <rect class="cursor" x="530" y="606" width="236" height="112" rx="56" fill="url(#lm-pill)" />
  </svg>
</template>

<style scoped>
.logo-mark {
  flex: none;
  border-radius: 26%;
  overflow: visible;
}
.chevron {
  transition: transform 0.5s var(--ease-spring);
  transform-origin: 374px 512px;
}
.animated .cursor {
  animation: blink 1.2s steps(1) infinite;
}
@keyframes blink {
  50% {
    opacity: 0.15;
  }
}
a:hover > .logo-mark .chevron {
  transform: translateX(28px);
}
</style>
