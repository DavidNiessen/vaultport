<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script setup lang="ts">
import { ArrowDown, BookOpen } from '@lucide/vue';
import { withBase } from 'vitepress';
</script>

<template>
  <section v-reveal class="cta-wrap" aria-labelledby="cta-title">
    <div class="cta">
      <div class="aura" aria-hidden="true" />
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

.aura {
  position: absolute;
  inset: 0;
  z-index: -1;
  background:
    radial-gradient(
      45% 70% at 20% 25%,
      rgb(var(--sc-glow-1) / calc(var(--sc-glow-strength) * 1.5)),
      transparent 72%
    ),
    radial-gradient(
      45% 70% at 82% 78%,
      rgb(var(--sc-glow-2) / calc(var(--sc-glow-strength) * 1.3)),
      transparent 72%
    );
  filter: blur(16px);
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
