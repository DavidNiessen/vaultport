<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script setup lang="ts">
/**
 * ⌘K search over VitePress' local search index (built at build time, loaded on first open).
 * Results are grouped by page, matched words are highlighted, and the keyboard drives it all.
 */
import localSearchIndex from '@localSearchIndex';
import { ArrowRight, CornerDownLeft, FileText, Hash, Search } from '@lucide/vue';
import MiniSearch, { type SearchResult } from 'minisearch';
import { useRouter, withBase } from 'vitepress';
import { computed, nextTick, onBeforeUnmount, onMounted, ref, shallowRef, watch } from 'vue';

import { sections } from '../../../nav';
import { searchOpen } from '../../composables/search';

interface Doc {
  title: string;
  titles: string[];
  text?: string;
}
type Hit = SearchResult & Doc;

const router = useRouter();
const input = ref<HTMLInputElement | null>(null);
const list = ref<HTMLElement | null>(null);
const query = ref('');
const active = ref(0);
const loading = ref(false);
const index = shallowRef<MiniSearch<Doc> | null>(null);

async function loadIndex() {
  if (index.value || loading.value) return;
  loading.value = true;
  try {
    const data = (await localSearchIndex.root?.())?.default;
    if (data) {
      index.value = MiniSearch.loadJSON<Doc>(data, {
        fields: ['title', 'titles', 'text'],
        storeFields: ['title', 'titles', 'text'],
        searchOptions: { fuzzy: 0.2, prefix: true, boost: { title: 4, text: 2, titles: 1 } },
      });
    }
  } finally {
    loading.value = false;
  }
}

/** The index holds HTML-escaped text; decode it once so it can be escaped again for display. */
function decode(text: string) {
  return text.replace(/&(amp|lt|gt|quot|#39|#x27);/g, (_, e: string) =>
    e === 'amp' ? '&' : e === 'lt' ? '<' : e === 'gt' ? '>' : e === 'quot' ? '"' : "'",
  );
}

const results = computed<Hit[]>(() => {
  const q = query.value.trim();
  if (!q || !index.value) return [];
  const hits = (index.value.search(q) as Hit[]).slice(0, 24).map((h) => ({
    ...h,
    title: decode(h.title),
    titles: h.titles.map(decode),
    text: h.text ? decode(h.text) : h.text,
  }));
  // Keep each page's sections together, pages in order of their best match.
  const order = new Map<string, number>();
  hits.forEach((h) => {
    const page = pageOf(h);
    if (!order.has(page)) order.set(page, order.size);
  });
  return hits
    .map((h, i) => ({ h, i }))
    .sort((a, b) => order.get(pageOf(a.h))! - order.get(pageOf(b.h))! || a.i - b.i)
    .map(({ h }) => h);
});

/** Quick links shown before typing. */
const suggestions = computed(() =>
  sections.slice(0, 4).flatMap((s) => s.items.slice(0, 2).map((i) => ({ ...i, section: s.text }))),
);

const itemCount = computed(() =>
  query.value.trim() ? results.value.length : suggestions.value.length,
);

function escapeHtml(s: string) {
  return s.replace(/[&<>"']/g, (c) => `&#${c.charCodeAt(0)};`);
}
function escapeRegExp(s: string) {
  return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}
function highlight(text: string, terms: string[]) {
  const safe = escapeHtml(text);
  if (!terms.length) return safe;
  const re = new RegExp(`(${terms.map((t) => escapeRegExp(escapeHtml(t))).join('|')})`, 'gi');
  return safe.replace(re, '<mark>$1</mark>');
}
/** A short window of the section text around the first match. */
function excerpt(hit: Hit) {
  const text = (hit.text ?? '').replace(/\s+/g, ' ').trim();
  if (!text) return '';
  const lower = text.toLowerCase();
  let at = -1;
  for (const t of hit.terms) {
    at = lower.indexOf(t.toLowerCase());
    if (at >= 0) break;
  }
  const start = Math.max(0, at - 40);
  const slice = text.slice(start, start + 150);
  return `${start > 0 ? '…' : ''}${highlight(slice, hit.terms)}${start + 150 < text.length ? '…' : ''}`;
}
function pageOf(hit: Hit) {
  return hit.titles[0] ?? hit.title;
}

function close() {
  searchOpen.value = false;
}
function go(link: string) {
  close();
  void router.go(link);
}
function choose(i: number) {
  if (query.value.trim()) {
    const hit = results.value[i];
    if (hit) go(String(hit.id));
  } else {
    const s = suggestions.value[i];
    if (s) go(withBase(s.link));
  }
}

function onKeydown(e: KeyboardEvent) {
  if (e.key === 'ArrowDown') {
    e.preventDefault();
    active.value = (active.value + 1) % Math.max(1, itemCount.value);
  } else if (e.key === 'ArrowUp') {
    e.preventDefault();
    active.value = (active.value - 1 + itemCount.value) % Math.max(1, itemCount.value);
  } else if (e.key === 'Enter') {
    e.preventDefault();
    choose(active.value);
  } else if (e.key === 'Escape') {
    e.preventDefault();
    close();
  }
}

watch(query, () => (active.value = 0));
watch(active, async () => {
  await nextTick();
  list.value?.querySelector('[data-active="true"]')?.scrollIntoView({ block: 'nearest' });
});
watch(searchOpen, async (open) => {
  document.documentElement.style.overflow = open ? 'hidden' : '';
  if (!open) return;
  void loadIndex();
  await nextTick();
  input.value?.focus();
  input.value?.select();
});

function onGlobalKey(e: KeyboardEvent) {
  const target = e.target as HTMLElement | null;
  const typing =
    target && (target.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(target.tagName));
  if ((e.key === 'k' || e.key === 'K') && (e.metaKey || e.ctrlKey)) {
    e.preventDefault();
    searchOpen.value = !searchOpen.value;
  } else if (e.key === '/' && !typing && !searchOpen.value) {
    e.preventDefault();
    searchOpen.value = true;
  }
}
onMounted(() => window.addEventListener('keydown', onGlobalKey));
onBeforeUnmount(() => window.removeEventListener('keydown', onGlobalKey));
</script>

<template>
  <Teleport to="body">
    <Transition name="palette">
      <div v-if="searchOpen" class="palette-backdrop" @mousedown.self="close">
        <div
          class="palette"
          role="dialog"
          aria-modal="true"
          aria-label="Search the documentation"
          @keydown="onKeydown"
        >
          <div class="field">
            <Search :size="18" class="field-icon" aria-hidden="true" />
            <input
              ref="input"
              v-model="query"
              type="search"
              placeholder="Search the docs…"
              aria-label="Search"
              autocomplete="off"
              spellcheck="false"
              role="combobox"
              aria-controls="palette-results"
              aria-expanded="true"
              :aria-activedescendant="itemCount ? `palette-item-${active}` : undefined"
            />
            <kbd class="esc" @click="close">esc</kbd>
            <span class="scan" :class="{ on: loading }" aria-hidden="true" />
          </div>

          <div id="palette-results" ref="list" class="results" role="listbox">
            <template v-if="!query.trim()">
              <p class="group-label">Jump to</p>
              <button
                v-for="(s, i) in suggestions"
                :id="`palette-item-${i}`"
                :key="s.link"
                type="button"
                role="option"
                class="item"
                :aria-selected="i === active"
                :data-active="i === active"
                @mousemove="active = i"
                @click="choose(i)"
              >
                <FileText :size="16" class="item-icon" aria-hidden="true" />
                <span class="item-main">
                  <span class="item-title">{{ s.text }}</span>
                  <span class="item-path">{{ s.section }}</span>
                </span>
                <ArrowRight :size="16" class="item-go" aria-hidden="true" />
              </button>
            </template>

            <template v-else-if="results.length">
              <template v-for="(hit, i) in results" :key="hit.id">
                <p v-if="i === 0 || pageOf(results[i - 1]!) !== pageOf(hit)" class="group-label">
                  {{ pageOf(hit) }}
                </p>
                <button
                  :id="`palette-item-${i}`"
                  type="button"
                  role="option"
                  class="item"
                  :aria-selected="i === active"
                  :data-active="i === active"
                  @mousemove="active = i"
                  @click="choose(i)"
                >
                  <Hash v-if="hit.titles.length" :size="16" class="item-icon" aria-hidden="true" />
                  <FileText v-else :size="16" class="item-icon" aria-hidden="true" />
                  <span class="item-main">
                    <!-- eslint-disable-next-line vue/no-v-html -- escaped in highlight() -->
                    <span class="item-title" v-html="highlight(hit.title, hit.terms)" />
                    <!-- eslint-disable-next-line vue/no-v-html -- escaped in excerpt() -->
                    <span v-if="hit.text" class="item-text" v-html="excerpt(hit)" />
                  </span>
                  <CornerDownLeft :size="16" class="item-go" aria-hidden="true" />
                </button>
              </template>
            </template>

            <div v-else class="empty">
              <p v-if="loading">Loading the index…</p>
              <template v-else>
                <p class="empty-title">No results for “{{ query }}”</p>
                <p>Try a protocol, a setting or a feature, like <em>jump host</em>.</p>
              </template>
            </div>
          </div>

          <footer class="foot">
            <span><kbd>↑</kbd><kbd>↓</kbd> to navigate</span>
            <span><kbd>↵</kbd> to open</span>
            <span class="foot-brand">Local search · nothing leaves your browser</span>
          </footer>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
.palette-backdrop {
  position: fixed;
  inset: 0;
  z-index: 120;
  display: flex;
  justify-content: center;
  align-items: flex-start;
  padding: min(12vh, 120px) 16px 16px;
  background: rgb(4 3 8 / 0.55);
  backdrop-filter: blur(8px);
  -webkit-backdrop-filter: blur(8px);
}
.palette {
  display: flex;
  flex-direction: column;
  width: min(640px, 100%);
  max-height: min(620px, 78vh);
  border-radius: 20px;
  background: color-mix(in oklab, var(--bg-elev) 94%, transparent);
  border: 1px solid var(--border-strong);
  box-shadow:
    var(--shadow-lg),
    0 0 120px -30px rgb(var(--glow-1) / 0.5);
  overflow: hidden;
}
.field {
  position: relative;
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 0 16px;
  border-bottom: 1px solid var(--border);
}
.field-icon {
  color: var(--accent);
  flex: none;
}
.field input {
  flex: 1;
  height: 60px;
  border: 0;
  outline: 0;
  background: none;
  font-size: 17px;
  letter-spacing: -0.01em;
}
.field input::placeholder {
  color: var(--text-3);
}
.field input::-webkit-search-cancel-button {
  display: none;
}
.esc {
  cursor: pointer;
}
.scan {
  position: absolute;
  left: 0;
  right: 0;
  bottom: -1px;
  height: 1px;
  background: linear-gradient(90deg, transparent, var(--accent), var(--accent-2), transparent);
  background-size: 50% 100%;
  background-repeat: no-repeat;
  opacity: 0;
}
.scan.on {
  opacity: 1;
  animation: scan 1s linear infinite;
}
@keyframes scan {
  from {
    background-position: -100% 0;
  }
  to {
    background-position: 200% 0;
  }
}
kbd {
  display: inline-grid;
  place-items: center;
  min-width: 22px;
  height: 22px;
  padding: 0 6px;
  border-radius: 6px;
  border: 1px solid var(--border);
  background: var(--surface);
  font-family: var(--font-mono);
  font-size: 11px;
  color: var(--text-3);
}
.results {
  flex: 1;
  overflow-y: auto;
  padding: 8px;
  overscroll-behavior: contain;
}
.group-label {
  padding: 12px 10px 6px;
  font-size: 11.5px;
  font-weight: 600;
  letter-spacing: 0.08em;
  text-transform: uppercase;
  color: var(--text-3);
}
.item {
  position: relative;
  display: flex;
  align-items: center;
  gap: 12px;
  width: 100%;
  padding: 10px 12px;
  border-radius: 12px;
  text-align: left;
  transition: background-color 0.15s;
}
.item[data-active='true'] {
  background: var(--accent-soft);
}
.item[data-active='true']::before {
  content: '';
  position: absolute;
  left: 0;
  top: 10px;
  bottom: 10px;
  width: 3px;
  border-radius: 3px;
  background: var(--accent);
  animation: grow 0.25s var(--ease-out);
}
@keyframes grow {
  from {
    transform: scaleY(0);
  }
}
.item-icon {
  flex: none;
  color: var(--text-3);
}
.item[data-active='true'] .item-icon {
  color: var(--accent);
}
.item-main {
  display: grid;
  gap: 2px;
  min-width: 0;
  flex: 1;
}
.item-title {
  font-size: 14.5px;
  font-weight: 550;
  color: var(--text);
}
.item-path,
.item-text {
  font-size: 13px;
  color: var(--text-3);
  overflow: hidden;
  text-overflow: ellipsis;
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
}
.item :deep(mark) {
  background: none;
  color: var(--accent);
  font-weight: 600;
  box-shadow: inset 0 -1px 0 color-mix(in oklab, var(--accent) 60%, transparent);
}
.item-go {
  flex: none;
  color: var(--text-3);
  opacity: 0;
  transform: translateX(-4px);
  transition:
    opacity 0.2s,
    transform 0.3s var(--ease-out);
}
.item[data-active='true'] .item-go {
  opacity: 1;
  transform: none;
}
.empty {
  padding: 40px 20px;
  text-align: center;
  color: var(--text-3);
  font-size: 14px;
}
.empty-title {
  color: var(--text);
  font-weight: 550;
  margin-bottom: 4px;
}
.foot {
  display: flex;
  align-items: center;
  gap: 16px;
  padding: 10px 16px;
  border-top: 1px solid var(--border);
  font-size: 12px;
  color: var(--text-3);
}
.foot kbd {
  margin-right: 4px;
}
.foot-brand {
  margin-left: auto;
}
@media (max-width: 600px) {
  .foot {
    display: none;
  }
  .palette-backdrop {
    padding-top: 12px;
  }
}

.palette-enter-active,
.palette-leave-active {
  transition: opacity 0.25s var(--ease-out);
}
.palette-enter-active .palette,
.palette-leave-active .palette {
  transition:
    transform 0.4s var(--ease-spring),
    opacity 0.25s var(--ease-out);
}
.palette-enter-from,
.palette-leave-to {
  opacity: 0;
}
.palette-enter-from .palette,
.palette-leave-to .palette {
  opacity: 0;
  transform: translateY(-12px) scale(0.96);
}
</style>
