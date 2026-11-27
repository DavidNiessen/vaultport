<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script setup lang="ts">
import { ArrowDown, BookOpen } from '@lucide/vue';
import { withBase } from 'vitepress';

import LivingGlow, { type GlowBlob } from './LivingGlow.vue';

const glow: GlowBlob[] = [
  {
    box: { top: '-30%', left: '-10%', width: '60%', height: '120%' },
    tone: 1,
    strength: 1.6,
    wander: 27,
    morph: 21,
    path: 1,
    delay: -3,
  },
  {
    box: { bottom: '-35%', right: '-12%', width: '60%', height: '120%' },
    tone: 2,
    strength: 1.35,
    wander: 33,
    morph: 25,
    path: 2,
    delay: -9,
  },
];
</script>

<template>
  <section v-reveal class="cta-wrap" aria-labelledby="cta-title">
    <div class="cta">
      <LivingGlow :blobs="glow" />
      <h2 id="cta-title" class="h2 sc-display">Take back<br />your keys.</h2>
      <p class="sub">Free and open source, on every device you use.</p>
      <div class="actions">
        <a class="sc-pill primary" :href="withBase('/download')">
          Download Vaultport <ArrowDown :size="18" aria-hidden="true" />
        </a>
        <a class="sc-pill ghost" :href="withBase('/guide/')">
          <BookOpen :size="18" aria-hidden="true" /> Read the docs
        </a>
      </div>
    </div>
  </section>
</template>

<style scoped>
.cta-wrap {
  max-width: 1200px;
  margin: 0 auto;
  padding: 120px 24px 112px;
}

.cta {
  position: relative;
  isolation: isolate;
  overflow: hidden;
  padding: 96px 32px;
  border-radius: 32px;
  background: var(--sc-surface);
  box-shadow: 0 0 0 1px var(--sc-border);
  text-align: center;
}

.h2 {
  margin: 0;
  color: var(--vp-c-text-1);
  font-size: clamp(40px, 7vw, 84px);
}

.sub {
  margin: 22px 0 0;
  color: var(--vp-c-text-2);
  font-size: 18px;
  font-weight: 600;
}

.actions {
  display: flex;
  flex-wrap: wrap;
  justify-content: center;
  gap: 12px;
  margin-top: 34px;
}

@media (max-width: 640px) {
  .cta {
    padding: 64px 20px;
  }
}
</style>
