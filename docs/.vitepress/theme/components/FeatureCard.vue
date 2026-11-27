<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script setup lang="ts">
/** A bento tile: icon, title, copy and a decorative visual. A soft glow follows the pointer. */
import { withBase } from 'vitepress';
import type { Component } from 'vue';

defineProps<{
  title: string;
  icon: Component;
  href?: string;
  delay?: number;
}>();

function track(event: PointerEvent) {
  const el = event.currentTarget as HTMLElement;
  const rect = el.getBoundingClientRect();
  el.style.setProperty('--mx', `${event.clientX - rect.left}px`);
  el.style.setProperty('--my', `${event.clientY - rect.top}px`);
}
</script>

<template>
  <article v-reveal="delay" class="card" @pointermove="track">
    <div class="visual" aria-hidden="true"><slot name="visual" /></div>
    <div class="copy">
      <h3 class="title">
        <span class="icon"><component :is="icon" :size="16" aria-hidden="true" /></span>
        {{ title }}
      </h3>
      <p class="text"><slot /></p>
      <a v-if="href" class="more" :href="withBase(href)">
        Learn more<span class="sr-only"> about {{ title }}</span> →
      </a>
    </div>
  </article>
</template>

<style scoped>
.card {
  position: relative;
  display: flex;
  flex-direction: column;
  gap: 20px;
  min-width: 0;
  padding: 24px;
  overflow: hidden;
  border-radius: var(--sc-radius);
  background:
    linear-gradient(
      180deg,
      color-mix(in srgb, var(--sc-surface-2) 80%, transparent),
      var(--sc-surface)
    ),
    var(--sc-surface);
  box-shadow:
    0 0 0 1px var(--sc-border),
    0 1px 0 0 rgb(255 255 255 / 0.04) inset;
  transition: box-shadow 0.3s var(--sc-ease);
}

.card::before {
  content: '';
  position: absolute;
  inset: 0;
  border-radius: inherit;
  background: radial-gradient(
    420px circle at var(--mx, 50%) var(--my, 0%),
    rgb(99 102 241 / 0.12),
    transparent 45%
  );
  opacity: 0;
  pointer-events: none;
  transition: opacity 0.3s;
}

.card:hover::before {
  opacity: 1;
}

.card:hover {
  box-shadow:
    0 0 0 1px color-mix(in srgb, var(--vp-c-brand-3) 45%, var(--sc-border)),
    0 1px 0 0 rgb(255 255 255 / 0.05) inset,
    0 24px 48px -24px rgb(99 102 241 / 0.35);
}

.visual {
  position: relative;
  flex: 1;
  min-height: 0;
}

.copy {
  position: relative;
}

.title {
  display: flex;
  align-items: center;
  gap: 10px;
  margin: 0;
  color: var(--vp-c-text-1);
  font-size: 17px;
  font-weight: 600;
  letter-spacing: -0.01em;
  line-height: 1.3;
}

.icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
  width: 28px;
  height: 28px;
  border-radius: 8px;
  background: var(--vp-c-brand-soft);
  box-shadow: inset 0 0 0 1px rgb(99 102 241 / 0.25);
  color: var(--sc-accent);
}

.text {
  margin: 8px 0 0;
  color: var(--vp-c-text-2);
  font-size: 14.5px;
  line-height: 1.6;
}

.more {
  display: inline-block;
  margin-top: 10px;
  color: var(--vp-c-brand-1);
  font-size: 13.5px;
  font-weight: 500;
  text-decoration: none;
}

.more:hover {
  text-decoration: underline;
}

@media (max-width: 640px) {
  .card {
    padding: 20px;
  }
}

@media (prefers-reduced-motion: reduce) {
  .card,
  .card::before {
    transition: none;
  }
}
</style>
