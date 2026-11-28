<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script setup lang="ts">
/**
 * Documentation page: sidebar, the Markdown content, "on this page" and the page footer
 * (edit link, last updated, previous/next). On small screens the sidebar becomes a drawer and
 * the outline a dropdown.
 */
import { ArrowLeft, ArrowRight, ChevronDown, PanelLeft, PencilLine, X } from '@lucide/vue';
import { onContentUpdated, useData, useRoute, withBase } from 'vitepress';
import { computed, nextTick, ref, watch } from 'vue';

import { sections } from '../../../nav';
import { REPO_URL } from '../../../site';
import DocOutline, { type Heading } from './DocOutline.vue';
import DocSidebar from './DocSidebar.vue';

const { page, frontmatter } = useData();
const route = useRoute();

const headings = ref<Heading[]>([]);
const drawerOpen = ref(false);
const outlineOpen = ref(false);

onContentUpdated(() => {
  const nodes = document.querySelectorAll<HTMLElement>('.prose :is(h2, h3)[id]');
  headings.value = [...nodes].map((n) => {
    const clone = n.cloneNode(true) as HTMLElement;
    clone.querySelectorAll('.header-anchor, .VPBadge').forEach((a) => a.remove());
    return { id: n.id, text: clone.textContent?.trim() ?? '', level: n.tagName === 'H3' ? 3 : 2 };
  });
});

watch(
  () => route.path,
  () => {
    drawerOpen.value = false;
    outlineOpen.value = false;
  },
);
watch(drawerOpen, async (open) => {
  document.documentElement.style.overflow = open ? 'hidden' : '';
  if (!open) return;
  await nextTick();
  document.querySelector<HTMLElement>('.drawer-close')?.focus();
});
function onDrawerKey(e: KeyboardEvent) {
  if (e.key === 'Escape') drawerOpen.value = false;
}

const flat = computed(() =>
  sections.flatMap((s) => s.items.map((i) => ({ ...i, section: s.text }))),
);
const currentIndex = computed(() => {
  const rel = '/' + page.value.relativePath.replace(/(^|\/)index\.md$/, '$1').replace(/\.md$/, '');
  return flat.value.findIndex((i) => i.link === rel);
});
const current = computed(() => flat.value[currentIndex.value]);
const prev = computed(() =>
  currentIndex.value > 0 ? flat.value[currentIndex.value - 1] : undefined,
);
const next = computed(() =>
  currentIndex.value >= 0 ? flat.value[currentIndex.value + 1] : undefined,
);

const editUrl = computed(() => `${REPO_URL}/edit/main/docs/${page.value.relativePath}`);
const lastUpdated = computed(() => {
  const ts = page.value.lastUpdated;
  if (!ts) return null;
  return {
    iso: new Date(ts).toISOString(),
    text: new Intl.DateTimeFormat('en', { dateStyle: 'medium', timeZone: 'UTC' }).format(
      new Date(ts),
    ),
  };
});
</script>

<template>
  <div class="doc-shell">
    <!-- Mobile toolbar -->
    <div class="doc-bar">
      <button type="button" class="bar-btn" @click="drawerOpen = true">
        <PanelLeft :size="16" aria-hidden="true" /> Menu
      </button>
      <button
        v-if="headings.length"
        type="button"
        class="bar-btn"
        :aria-expanded="outlineOpen"
        @click="outlineOpen = !outlineOpen"
      >
        On this page
        <ChevronDown
          :size="15"
          class="bar-chev"
          :class="{ open: outlineOpen }"
          aria-hidden="true"
        />
      </button>
      <Transition name="drop">
        <div v-if="outlineOpen" class="outline-drop">
          <a
            v-for="h in headings"
            :key="h.id"
            :href="`#${h.id}`"
            :class="`lvl-${h.level}`"
            @click="outlineOpen = false"
            >{{ h.text }}</a
          >
        </div>
      </Transition>
    </div>

    <aside class="doc-sidebar" aria-label="Documentation navigation">
      <DocSidebar />
    </aside>

    <Transition name="drawer">
      <div
        v-if="drawerOpen"
        class="drawer-backdrop"
        @click.self="drawerOpen = false"
        @keydown="onDrawerKey"
      >
        <div class="drawer" role="dialog" aria-modal="true" aria-label="Documentation navigation">
          <button
            type="button"
            class="drawer-close"
            aria-label="Close menu"
            @click="drawerOpen = false"
          >
            <X :size="18" />
          </button>
          <DocSidebar />
        </div>
      </div>
    </Transition>

    <article :key="route.path" class="doc-main page-enter">
      <p v-if="current" class="crumbs">
        <span>{{ current.section }}</span>
        <span class="crumb-sep" aria-hidden="true">/</span>
        <span class="crumb-current">{{ current.text }}</span>
      </p>
      <Content class="prose" />

      <footer class="doc-foot">
        <div class="meta">
          <a :href="editUrl" target="_blank" rel="noreferrer" class="edit">
            <PencilLine :size="15" aria-hidden="true" /> Edit this page on GitHub
          </a>
          <span v-if="lastUpdated && frontmatter.lastUpdated !== false" class="updated">
            Last updated <time :datetime="lastUpdated.iso">{{ lastUpdated.text }}</time>
          </span>
        </div>
        <nav v-if="prev || next" class="pager" aria-label="Previous and next pages">
          <a v-if="prev" :href="withBase(prev.link)" class="pager-card prev">
            <span class="pager-label"><ArrowLeft :size="14" aria-hidden="true" /> Previous</span>
            <span class="pager-title">{{ prev.text }}</span>
          </a>
          <span v-else />
          <a v-if="next" :href="withBase(next.link)" class="pager-card next">
            <span class="pager-label">Next <ArrowRight :size="14" aria-hidden="true" /></span>
            <span class="pager-title">{{ next.text }}</span>
          </a>
        </nav>
      </footer>
    </article>

    <aside class="doc-outline">
      <DocOutline :headings="headings" />
    </aside>
  </div>
</template>

<style scoped>
.doc-shell {
  --sidebar-w: 272px;
  --outline-w: 220px;
  position: relative;
  display: grid;
  grid-template-columns: var(--sidebar-w) minmax(0, 1fr) var(--outline-w);
  gap: clamp(32px, 4vw, 64px);
  max-width: 1440px;
  margin: 0 auto;
  padding: calc(var(--nav-h) + 32px) clamp(16px, 2.4vw, 32px) 80px;
}
/* A soft glow behind the page header */
.doc-shell::before {
  content: '';
  position: absolute;
  inset: 0 0 auto;
  height: 520px;
  z-index: -1;
  pointer-events: none;
  background:
    radial-gradient(40% 60% at 30% 0%, rgb(var(--glow-1) / 0.16), transparent 70%),
    radial-gradient(30% 50% at 75% 0%, rgb(var(--glow-2) / 0.1), transparent 70%);
}
.doc-sidebar,
.doc-outline {
  position: sticky;
  top: calc(var(--nav-h) + 24px);
  align-self: start;
  max-height: calc(100vh - var(--nav-h) - 40px);
  overflow-y: auto;
  overscroll-behavior: contain;
}
.doc-sidebar {
  margin-left: -10px;
  padding-right: 6px;
  mask-image: linear-gradient(to bottom, #000 calc(100% - 40px), transparent);
}
.doc-main {
  min-width: 0;
  max-width: 780px;
  width: 100%;
  margin: 0 auto;
}
.crumbs {
  display: flex;
  gap: 8px;
  margin-bottom: 18px;
  font-family: var(--font-mono);
  font-size: 12px;
  letter-spacing: 0.04em;
  text-transform: uppercase;
  color: var(--accent);
}
.crumb-sep {
  color: var(--text-3);
}
.crumb-current {
  color: var(--text-3);
}

.doc-foot {
  margin-top: 72px;
  padding-top: 24px;
  border-top: 1px solid var(--border);
}
.meta {
  display: flex;
  flex-wrap: wrap;
  justify-content: space-between;
  gap: 12px;
  font-size: 13.5px;
  color: var(--text-3);
}
.edit {
  min-height: 28px;
  display: inline-flex;
  align-items: center;
  gap: 8px;
  color: var(--accent);
  font-weight: 500;
}
.edit:hover {
  text-decoration: underline;
  text-underline-offset: 3px;
}
.pager {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 16px;
  margin-top: 32px;
}
.pager-card {
  position: relative;
  display: grid;
  gap: 4px;
  padding: 18px 20px;
  border-radius: var(--radius);
  border: 1px solid var(--border);
  background: var(--surface);
  overflow: hidden;
  transition:
    border-color 0.3s,
    transform 0.4s var(--ease-out),
    background-color 0.3s;
}
.pager-card::after {
  content: '';
  position: absolute;
  inset: auto 0 0;
  height: 2px;
  background: linear-gradient(90deg, var(--accent), var(--accent-2));
  transform: scaleX(0);
  transform-origin: left;
  transition: transform 0.5s var(--ease-out);
}
.pager-card.next {
  text-align: right;
}
.pager-card.next::after {
  transform-origin: right;
}
.pager-card:hover {
  border-color: var(--border-strong);
  background: var(--surface-2);
  transform: translateY(-2px);
}
.pager-card:hover::after {
  transform: scaleX(1);
}
.pager-label {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-size: 12.5px;
  color: var(--text-3);
}
.next .pager-label {
  justify-content: flex-end;
}
.pager-title {
  font-weight: 600;
  font-size: 16px;
  letter-spacing: -0.015em;
  color: var(--text);
}

/* Mobile toolbar and drawer */
.doc-bar {
  display: none;
}
.drawer-backdrop {
  position: fixed;
  inset: 0;
  z-index: 60;
  background: rgb(4 3 8 / 0.5);
  backdrop-filter: blur(4px);
}
.drawer {
  position: absolute;
  inset: 0 auto 0 0;
  width: min(320px, 86vw);
  padding: 20px 16px;
  overflow-y: auto;
  background: var(--bg-elev);
  border-right: 1px solid var(--border);
  box-shadow: var(--shadow-lg);
}
.drawer-close {
  display: grid;
  place-items: center;
  width: 36px;
  height: 36px;
  margin: 0 0 8px auto;
  border-radius: 999px;
  color: var(--text-2);
}
.drawer-close:hover {
  background: var(--surface-2);
}
.drawer-enter-active,
.drawer-leave-active {
  transition: opacity 0.3s;
}
.drawer-enter-active .drawer,
.drawer-leave-active .drawer {
  transition: transform 0.45s var(--ease-out);
}
.drawer-enter-from,
.drawer-leave-to {
  opacity: 0;
}
.drawer-enter-from .drawer,
.drawer-leave-to .drawer {
  transform: translateX(-100%);
}

@media (max-width: 1280px) {
  .doc-shell {
    grid-template-columns: var(--sidebar-w) minmax(0, 1fr);
  }
  .doc-outline {
    display: none;
  }
}
@media (max-width: 1280px) and (min-width: 961px) {
  .doc-bar {
    display: flex;
    grid-column: 2;
    justify-content: flex-end;
  }
  .doc-bar .bar-btn:first-child {
    display: none;
  }
}
@media (max-width: 960px) {
  .doc-shell {
    grid-template-columns: minmax(0, 1fr);
    padding-top: var(--nav-h);
  }
  .doc-sidebar {
    display: none;
  }
  .doc-bar {
    position: sticky;
    top: var(--nav-h);
    z-index: 20;
    display: flex;
    justify-content: space-between;
    margin: 0 calc(-1 * clamp(16px, 2.4vw, 32px));
    padding: 8px clamp(16px, 2.4vw, 32px);
    background: color-mix(in oklab, var(--bg) 80%, transparent);
    backdrop-filter: blur(16px);
    -webkit-backdrop-filter: blur(16px);
    border-bottom: 1px solid var(--border);
  }
  .doc-main {
    padding-top: 36px;
  }
  .pager {
    grid-template-columns: 1fr;
  }
}
.doc-bar {
  position: relative;
}
.bar-btn {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  height: 36px;
  padding: 0 12px;
  border-radius: 999px;
  font-size: 13.5px;
  font-weight: 500;
  color: var(--text-2);
}
.bar-btn:hover {
  background: var(--surface-2);
  color: var(--text);
}
.bar-chev {
  transition: transform 0.3s var(--ease-out);
}
.bar-chev.open {
  transform: rotate(180deg);
}
.outline-drop {
  position: absolute;
  right: 12px;
  top: calc(100% + 6px);
  z-index: 30;
  display: grid;
  width: min(320px, calc(100vw - 32px));
  max-height: 60vh;
  overflow-y: auto;
  padding: 8px;
  border-radius: var(--radius);
  border: 1px solid var(--border-strong);
  background: var(--bg-elev);
  box-shadow: var(--shadow-lg);
}
.outline-drop a {
  padding: 7px 10px;
  border-radius: 8px;
  font-size: 14px;
  color: var(--text-2);
}
.outline-drop a.lvl-3 {
  padding-left: 22px;
  font-size: 13px;
}
.outline-drop a:hover {
  background: var(--surface-2);
  color: var(--text);
}
.drop-enter-active,
.drop-leave-active {
  transition:
    opacity 0.2s,
    transform 0.3s var(--ease-out);
}
.drop-enter-from,
.drop-leave-to {
  opacity: 0;
  transform: translateY(-6px) scale(0.98);
}
</style>
