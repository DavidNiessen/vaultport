<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script setup lang="ts">
/**
 * Soft, slowly moving lights in the site colours, like the app background: each light wanders
 * along its own path while turning and stretching, with different durations, so the shapes
 * keep changing. Only transform is animated; motion stops with prefers-reduced-motion.
 * Styles live in ../style.css (`.sc-glow`).
 */
import type { CSSProperties } from 'vue';

export interface GlowBlob {
  box: CSSProperties;
  tone: 1 | 2;
  strength: number;
  wander: number;
  morph: number;
  path: 1 | 2 | 3;
  delay: number;
}

defineProps<{ blobs: GlowBlob[] }>();
</script>

<template>
  <div class="sc-glow" aria-hidden="true">
    <div
      v-for="(b, i) in blobs"
      :key="i"
      :class="['sc-glow-wander', `sc-glow-path-${b.path}`]"
      :style="{ ...b.box, animationDuration: `${b.wander}s`, animationDelay: `${b.delay}s` }"
    >
      <div
        class="sc-glow-blob"
        :style="{
          '--blob-color': `rgb(var(--sc-glow-${b.tone}) / calc(var(--sc-glow-strength) * ${b.strength}))`,
          animationDuration: `${b.morph}s`,
          animationDelay: `${b.delay / 2}s`,
        }"
      />
    </div>
  </div>
</template>
