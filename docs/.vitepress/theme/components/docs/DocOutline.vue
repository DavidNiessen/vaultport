<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script setup lang="ts">
/**
 * "On this page": headings of the current page with scroll spy. A bright segment glides along
 * the rail to the section being read, and a ring shows how much of the page is read.
 */
import { ArrowUp } from '@lucide/vue';
import { onContentUpdated } from 'vitepress';
import { computed, nextTick, onBeforeUnmount, onMounted, reactive, ref } from 'vue';

export interface Heading {
  id: string;
  text: string;
  level: number;
}

const props = defineProps<{ headings: Heading[] }>();
const activeId = ref<string | null>(null);
const progress = ref(0);
const listEl = ref<HTMLElement | null>(null);
const marker = reactive({ y: 0, h: 0 });

const activeIndex = computed(() => props.headings.findIndex((h) => h.id === activeId.value));

let frame = 0;
function update() {
  frame = 0;
  const offset = 120;
  let current: string | null = null;
  for (const h of props.headings) {
    const el = document.getElementById(h.id);
    if (el && el.getBoundingClientRect().top - offset <= 0) current = h.id;
  }
  const atBottom = window.innerHeight + window.scrollY >= document.body.scrollHeight - 4;
  if (atBottom && props.headings.length) current = props.headings[props.headings.length - 1]!.id;
  activeId.value = current ?? props.headings[0]?.id ?? null;
  const max = document.documentElement.scrollHeight - window.innerHeight;
  progress.value = max > 0 ? Math.min(1, window.scrollY / max) : 0;
  void nextTick(placeMarker);
}
function onScroll() {
  if (!frame) frame = requestAnimationFrame(update);
}
function placeMarker() {
  const el = listEl.value?.querySelector<HTMLElement>(
    `[data-id="${CSS.escape(activeId.value ?? '')}"]`,
  );
  if (!el) return;
  marker.y = el.offsetTop;
  marker.h = el.offsetHeight;
}

onMounted(() => {
  update();
  window.addEventListener('scroll', onScroll, { passive: true });
});
onContentUpdated(() => void nextTick(update));
onBeforeUnmount(() => {
  cancelAnimationFrame(frame);
  window.removeEventListener('scroll', onScroll);
});

const circumference = 2 * Math.PI * 9;
function toTop() {
  window.scrollTo({ top: 0, behavior: 'smooth' });
}
</script>

<template>
  <nav v-if="headings.length" class="outline" aria-label="On this page">
    <p class="title">On this page</p>
    <div class="rail-wrap">
      <span class="rail" aria-hidden="true" />
      <span
        class="rail-active"
        :style="{
          transform: `translateY(${marker.y}px)`,
          height: `${marker.h}px`,
          opacity: activeIndex >= 0 ? 1 : 0,
        }"
        aria-hidden="true"
      />
      <ul ref="listEl">
        <li v-for="h in headings" :key="h.id" :data-id="h.id" :class="`lvl-${h.level}`">
          <a :href="`#${h.id}`" :class="{ active: h.id === activeId }">{{ h.text }}</a>
        </li>
      </ul>
    </div>
    <a href="#" class="top" @click.prevent="toTop">
      <svg viewBox="0 0 22 22" width="22" height="22" class="ring" aria-hidden="true">
        <circle cx="11" cy="11" r="9" class="ring-bg" />
        <circle
          cx="11"
          cy="11"
          r="9"
          class="ring-fg"
          :style="{
            strokeDasharray: circumference,
            strokeDashoffset: circumference * (1 - progress),
          }"
        />
      </svg>
      <ArrowUp :size="12" class="top-arrow" aria-hidden="true" />
      Back to top
    </a>
  </nav>
</template>

<style scoped>
.outline {
  font-size: 13px;
}
.title {
  font-size: 12px;
  font-weight: 600;
  letter-spacing: 0.08em;
  text-transform: uppercase;
  color: var(--text-3);
  margin-bottom: 12px;
}
.rail-wrap {
  position: relative;
  padding-left: 14px;
}
.rail {
  position: absolute;
  left: 0;
  top: 0;
  bottom: 0;
  width: 1px;
  background: var(--border);
}
.rail-active {
  position: absolute;
  left: -0.5px;
  top: 0;
  width: 2px;
  border-radius: 2px;
  background: linear-gradient(var(--accent), var(--accent-2));
  box-shadow: 0 0 10px var(--accent);
  transition:
    transform 0.45s var(--ease-out),
    height 0.45s var(--ease-out),
    opacity 0.3s;
}
ul {
  list-style: none;
  margin: 0;
  padding: 0;
}
li a {
  display: block;
  padding: 5px 0;
  line-height: 1.45;
  color: var(--text-3);
  overflow-wrap: anywhere;
  transition: color 0.2s;
}
li.lvl-3 a {
  padding-left: 12px;
  font-size: 12.5px;
}
li a:hover {
  color: var(--text-2);
}
li a.active {
  color: var(--text);
}
.top {
  min-height: 28px;
  display: inline-flex;
  align-items: center;
  gap: 8px;
  margin-top: 20px;
  color: var(--text-3);
  position: relative;
}
.top:hover {
  color: var(--text);
}
.ring {
  transform: rotate(-90deg);
}
.ring circle {
  fill: none;
  stroke-width: 2;
}
.ring-bg {
  stroke: var(--border);
}
.ring-fg {
  stroke: var(--accent);
  stroke-linecap: round;
  transition: stroke-dashoffset 0.15s linear;
}
.top-arrow {
  position: absolute;
  left: 5px;
}
</style>
