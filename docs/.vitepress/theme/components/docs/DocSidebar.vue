<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script setup lang="ts">
/**
 * Docs navigation. Sections collapse, and a glowing marker glides to the current page whenever
 * the route changes.
 */
import {
  BookOpen,
  ChevronDown,
  Code2,
  Compass,
  LifeBuoy,
  Lightbulb,
  ShieldCheck,
} from '@lucide/vue';
import { useRoute, withBase } from 'vitepress';
import { nextTick, onMounted, reactive, ref, watch, type Component } from 'vue';

import { sections } from '../../../nav';

const route = useRoute();
const root = ref<HTMLElement | null>(null);
const marker = reactive({ y: 0, h: 0, visible: false });

const ICONS: Record<string, Component> = {
  'Getting started': Compass,
  Concepts: Lightbulb,
  Guides: BookOpen,
  Security: ShieldCheck,
  Development: Code2,
  Help: LifeBuoy,
};

const collapsed = reactive<Record<string, boolean>>({});

function normalize(p: string) {
  return p.replace(/\.html$/, '').replace(/\/index$/, '/');
}
function isActive(link: string) {
  return normalize(route.path) === normalize(withBase(link));
}

function placeMarker() {
  const el = root.value?.querySelector<HTMLElement>('a.item.active');
  if (!el || !root.value) {
    marker.visible = false;
    return;
  }
  marker.y = el.offsetTop;
  marker.h = el.offsetHeight;
  marker.visible = true;
}

function toggle(text: string) {
  collapsed[text] = !collapsed[text];
  // Wait for the collapse animation before moving the marker.
  setTimeout(placeMarker, 360);
}

onMounted(() => {
  placeMarker();
  root.value?.querySelector('a.item.active')?.scrollIntoView({ block: 'center' });
});
watch(
  () => route.path,
  async () => {
    await nextTick();
    // Open the section that holds the new page.
    for (const s of sections) if (s.items.some((i) => isActive(i.link))) collapsed[s.text] = false;
    await nextTick();
    placeMarker();
  },
);
</script>

<template>
  <nav ref="root" class="sidebar" aria-label="Documentation">
    <span
      class="marker"
      :style="{
        transform: `translateY(${marker.y}px)`,
        height: `${marker.h}px`,
        opacity: marker.visible ? 1 : 0,
      }"
      aria-hidden="true"
    />
    <section
      v-for="s in sections"
      :key="s.text"
      class="group"
      :class="{ closed: collapsed[s.text] }"
    >
      <button
        type="button"
        class="group-head"
        :aria-expanded="!collapsed[s.text]"
        @click="toggle(s.text)"
      >
        <component
          :is="ICONS[s.text] ?? BookOpen"
          :size="15"
          class="group-icon"
          aria-hidden="true"
        />
        <span>{{ s.text }}</span>
        <ChevronDown :size="14" class="chev" aria-hidden="true" />
      </button>
      <div class="group-body">
        <ul>
          <li v-for="item in s.items" :key="item.link">
            <a
              :href="withBase(item.link)"
              class="item"
              :class="{ active: isActive(item.link) }"
              :aria-current="isActive(item.link) ? 'page' : undefined"
              >{{ item.text }}</a
            >
          </li>
        </ul>
      </div>
    </section>
  </nav>
</template>

<style scoped>
.sidebar {
  position: relative;
  display: grid;
  gap: 6px;
  padding: 8px 0 40px;
}
.marker {
  position: absolute;
  left: 0;
  right: 0;
  top: 0;
  border-radius: 10px;
  background: var(--accent-soft);
  pointer-events: none;
  transition:
    transform 0.5s var(--ease-out),
    height 0.5s var(--ease-out),
    opacity 0.3s;
}
.marker::before {
  content: '';
  position: absolute;
  left: 0;
  top: 8px;
  bottom: 8px;
  width: 2px;
  border-radius: 2px;
  background: var(--accent);
  box-shadow: 0 0 12px var(--accent);
}
.group-head {
  display: flex;
  align-items: center;
  gap: 10px;
  width: 100%;
  padding: 8px 10px;
  font-size: 13px;
  font-weight: 600;
  color: var(--text);
  letter-spacing: -0.005em;
  border-radius: 8px;
}
.group-head:hover {
  background: var(--surface);
}
.group-icon {
  color: var(--accent);
}
.chev {
  margin-left: auto;
  color: var(--text-3);
  transition: transform 0.35s var(--ease-out);
}
.closed .chev {
  transform: rotate(-90deg);
}
.group-body {
  display: grid;
  grid-template-rows: 1fr;
  transition: grid-template-rows 0.35s var(--ease-out);
}
.closed .group-body {
  grid-template-rows: 0fr;
}
.group-body > ul {
  min-height: 0;
  overflow: hidden;
  list-style: none;
  margin: 0;
  padding: 0 0 6px;
}
.item {
  position: relative;
  display: block;
  padding: 6px 12px 6px 35px;
  border-radius: 10px;
  font-size: 14px;
  color: var(--text-2);
  transition:
    color 0.2s,
    transform 0.3s var(--ease-out);
}
.item:hover {
  color: var(--text);
  transform: translateX(2px);
}
.item.active {
  color: var(--text);
  font-weight: 550;
}
</style>
