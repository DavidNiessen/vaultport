<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script setup lang="ts">
import { BookOpen, Download } from '@lucide/vue';
import { withBase } from 'vitepress';
</script>

<template>
  <section v-reveal class="cta" aria-labelledby="cta-title">
    <img class="logo" :src="withBase('/logo.svg')" alt="" width="64" height="64" />
    <h2 id="cta-title" class="h2">Your servers. Your keys. Your storage.</h2>
    <p class="sub">Free and open source, on every device you use.</p>
    <div class="actions">
      <a class="btn primary" :href="withBase('/download')">
        <Download :size="18" aria-hidden="true" /> Download Vaultport
      </a>
      <a class="btn secondary" :href="withBase('/guide/')">
        <BookOpen :size="18" aria-hidden="true" /> Read the docs
      </a>
    </div>
  </section>
</template>

<style scoped>
.cta {
  max-width: 1152px;
  margin: 0 auto;
  padding: 112px 24px 120px;
  text-align: center;
}

.logo {
  display: block;
  margin: 0 auto 24px;
  filter: drop-shadow(0 16px 32px rgb(99 102 241 / 0.45));
}

.h2 {
  margin: 0;
  color: var(--vp-c-text-1);
  font-size: clamp(28px, 4vw, 44px);
  font-weight: 700;
  letter-spacing: -0.03em;
  line-height: 1.15;
  text-wrap: balance;
}

.sub {
  margin: 12px 0 0;
  color: var(--vp-c-text-2);
  font-size: 17px;
}

.actions {
  display: flex;
  flex-wrap: wrap;
  justify-content: center;
  gap: 12px;
  margin-top: 32px;
}

.btn {
  display: inline-flex;
  align-items: center;
  gap: 10px;
  height: 46px;
  padding: 0 20px;
  border-radius: 12px;
  font-size: 15px;
  font-weight: 600;
  text-decoration: none;
  transition: transform 0.2s var(--sc-ease);
}

.btn:hover {
  transform: translateY(-1px);
}

.btn.primary {
  background: #6366f1;
  color: #fff;
  box-shadow: 0 12px 32px -12px rgb(99 102 241 / 0.8);
}

.btn.secondary {
  background: var(--vp-c-bg-soft);
  color: var(--vp-c-text-1);
  box-shadow: 0 0 0 1px var(--sc-border-strong);
}

@media (max-width: 640px) {
  .cta {
    padding: 80px 16px 88px;
  }
}

@media (prefers-reduced-motion: reduce) {
  .btn {
    transition: none;
  }
}
</style>
