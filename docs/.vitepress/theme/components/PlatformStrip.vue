<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script setup lang="ts">
import { Laptop, Monitor, Smartphone, Tablet, SquareTerminal } from '@lucide/vue';
import { withBase } from 'vitepress';

const platforms = [
  { id: 'windows', name: 'Windows', detail: '10 & 11 · x64, ARM64', icon: Monitor },
  { id: 'macos', name: 'macOS', detail: '13+ · Apple silicon & Intel', icon: Laptop },
  { id: 'linux', name: 'Linux', detail: 'AppImage · deb · rpm', icon: SquareTerminal },
  { id: 'ios', name: 'iOS & iPadOS', detail: '16+ · phone & tablet UI', icon: Tablet },
  { id: 'android', name: 'Android', detail: '8.0+ · phone & tablet UI', icon: Smartphone },
];
</script>

<template>
  <section class="strip" aria-labelledby="platforms-title">
    <h2 id="platforms-title" class="sr-only">Supported platforms</h2>
    <ul class="list">
      <li v-for="(p, i) in platforms" :key="p.id" v-reveal="i * 60">
        <a class="item" :href="withBase(`/download#${p.id}`)">
          <component :is="p.icon" :size="20" aria-hidden="true" class="icon" />
          <span class="text">
            <span class="name">{{ p.name }}</span>
            <span class="detail">{{ p.detail }}</span>
          </span>
        </a>
      </li>
    </ul>
    <p class="note">One Rust core and one UI on every platform. Same vault, same features.</p>
  </section>
</template>

<style scoped>
.strip {
  max-width: 1152px;
  margin: 0 auto;
  padding: 24px 24px 8px;
}

.list {
  display: grid;
  grid-template-columns: repeat(5, minmax(0, 1fr));
  gap: 12px;
  margin: 0;
  padding: 0;
  list-style: none;
}

.item {
  display: flex;
  align-items: center;
  gap: 12px;
  height: 100%;
  padding: 14px 16px;
  border-radius: 14px;
  background: color-mix(in srgb, var(--sc-surface) 70%, transparent);
  box-shadow: 0 0 0 1px var(--sc-border);
  color: inherit;
  text-decoration: none;
  transition:
    box-shadow 0.2s,
    transform 0.2s var(--sc-ease);
}

.item:hover {
  box-shadow: 0 0 0 1px var(--vp-c-brand-2);
  transform: translateY(-2px);
}

.icon {
  flex-shrink: 0;
  color: var(--sc-accent);
}

.text {
  display: grid;
  min-width: 0;
}

.name {
  color: var(--vp-c-text-1);
  font-size: 14px;
  font-weight: 600;
}

.detail {
  color: var(--vp-c-text-3);
  font-size: 12px;
}

.note {
  margin: 16px 0 0;
  color: var(--vp-c-text-3);
  font-size: 13px;
  text-align: center;
}

@media (max-width: 960px) {
  .list {
    grid-template-columns: repeat(3, minmax(0, 1fr));
  }
}

@media (max-width: 640px) {
  .strip {
    padding: 16px 16px 0;
  }
  .list {
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 8px;
  }
  .list li:last-child {
    grid-column: 1 / -1;
  }
  .item {
    padding: 12px;
  }
}

@media (prefers-reduced-motion: reduce) {
  .item {
    transition: none;
  }
}
</style>
