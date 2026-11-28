<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script setup lang="ts">
/**
 * Top navigation. On marketing pages it floats as a glass pill that tucks away while scrolling
 * down and returns on the way up; in the docs it is a full-width bar. A highlight slides between
 * links under the pointer. Phones get a full-screen menu with staggered links.
 */
import { Menu, Search, X } from '@lucide/vue';
import { useRoute, withBase } from 'vitepress';
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue';

import { navLinks } from '../../../nav';
import { REPO_URL } from '../../../site';
import { detectModKey, modKey, openSearch } from '../../composables/search';
import GithubIcon from './GithubIcon.vue';
import LogoMark from './LogoMark.vue';
import ThemeToggle from './ThemeToggle.vue';

const props = defineProps<{ variant: 'landing' | 'doc' | 'notfound' }>();
const route = useRoute();

const scrolled = ref(false);
const hidden = ref(false);
const menuOpen = ref(false);

const activeIndex = computed(() => {
  const path = route.path.replace(withBase('/'), '/');
  return navLinks.findIndex((l) => l.match.test(path));
});

/* Sliding hover highlight */
const linksEl = ref<HTMLElement | null>(null);
const pill = ref({ x: 0, w: 0, visible: false });
function hover(e: PointerEvent | FocusEvent) {
  const target = e.currentTarget as HTMLElement;
  pill.value = { x: target.offsetLeft, w: target.offsetWidth, visible: true };
}
function unhover() {
  pill.value = { ...pill.value, visible: false };
}

let lastY = 0;
let frame = 0;
function onScroll() {
  if (frame) return;
  frame = requestAnimationFrame(() => {
    frame = 0;
    const y = window.scrollY;
    scrolled.value = y > 12;
    // Tuck the floating nav away while reading downwards on marketing pages.
    hidden.value = props.variant === 'landing' && y > 400 && y > lastY + 4 && !menuOpen.value;
    if (y < lastY - 4 || y < 400) hidden.value = false;
    lastY = y;
  });
}

function onKey(e: KeyboardEvent) {
  if (e.key === 'Escape') menuOpen.value = false;
}

onMounted(() => {
  detectModKey();
  onScroll();
  window.addEventListener('scroll', onScroll, { passive: true });
  window.addEventListener('keydown', onKey);
});
onBeforeUnmount(() => {
  window.removeEventListener('scroll', onScroll);
  window.removeEventListener('keydown', onKey);
});

watch(
  () => route.path,
  () => {
    menuOpen.value = false;
    void nextTick(onScroll);
  },
);
watch(menuOpen, (open) => {
  document.documentElement.style.overflow = open ? 'hidden' : '';
});
</script>

<template>
  <header
    class="nav"
    :class="[`nav--${variant === 'doc' ? 'doc' : 'landing'}`, { scrolled, hidden, open: menuOpen }]"
  >
    <div class="nav-inner">
      <a :href="withBase('/')" class="brand" aria-label="Vaultport home">
        <LogoMark :size="28" />
        <span class="wordmark">Vaultport</span>
      </a>

      <nav ref="linksEl" class="links" aria-label="Main" @pointerleave="unhover">
        <span
          class="hover-pill"
          :style="{
            transform: `translateX(${pill.x}px)`,
            width: `${pill.w}px`,
            opacity: pill.visible ? 1 : 0,
          }"
          aria-hidden="true"
        />
        <a
          v-for="(l, i) in navLinks"
          :key="l.link"
          :href="withBase(l.link)"
          class="link"
          :class="{ active: i === activeIndex }"
          :aria-current="i === activeIndex ? 'page' : undefined"
          @pointerenter="hover"
          @focus="hover"
          @blur="unhover"
          >{{ l.text }}</a
        >
      </nav>

      <div class="actions">
        <button type="button" class="search-btn" aria-label="Search the docs" @click="openSearch">
          <Search :size="16" aria-hidden="true" />
          <span class="search-label">Search</span>
          <kbd v-if="modKey" class="search-kbd">{{ modKey }} K</kbd>
        </button>
        <ThemeToggle />
        <a
          class="icon-btn gh"
          :href="REPO_URL"
          target="_blank"
          rel="noreferrer"
          aria-label="Vaultport on GitHub"
        >
          <GithubIcon />
        </a>
        <button
          type="button"
          class="icon-btn menu-btn"
          :aria-expanded="menuOpen"
          aria-controls="mobile-menu"
          :aria-label="menuOpen ? 'Close menu' : 'Open menu'"
          @click="menuOpen = !menuOpen"
        >
          <X v-if="menuOpen" :size="20" aria-hidden="true" />
          <Menu v-else :size="20" aria-hidden="true" />
        </button>
      </div>
    </div>
  </header>

  <Transition name="menu">
    <div v-if="menuOpen" id="mobile-menu" class="mobile-menu" role="dialog" aria-label="Menu">
      <nav aria-label="Main">
        <a
          v-for="(l, i) in [{ text: 'Home', link: '/' }, ...navLinks]"
          :key="l.link"
          :href="withBase(l.link)"
          class="m-link"
          :style="{ '--i': i }"
        >
          <span class="m-index mono">0{{ i + 1 }}</span>
          {{ l.text }}
        </a>
      </nav>
      <div class="m-foot" :style="{ '--i': navLinks.length + 1 }">
        <a :href="REPO_URL" target="_blank" rel="noreferrer" class="btn"
          ><GithubIcon /> Star on GitHub</a
        >
      </div>
    </div>
  </Transition>
</template>

<style scoped>
.nav {
  position: fixed;
  inset: 0 0 auto;
  z-index: 50;
  height: var(--nav-h);
  transition:
    transform 0.5s var(--ease-out),
    background-color 0.3s,
    border-color 0.3s;
}
.nav-inner {
  display: flex;
  align-items: center;
  gap: 12px;
  height: 100%;
}

/* Docs: a full-width bar */
.nav--doc {
  border-bottom: 1px solid transparent;
  background: color-mix(in oklab, var(--bg) 70%, transparent);
  backdrop-filter: blur(16px) saturate(1.4);
  -webkit-backdrop-filter: blur(16px) saturate(1.4);
}
.nav--doc.scrolled {
  border-bottom-color: var(--border);
}
.nav--doc .nav-inner {
  padding: 0 clamp(16px, 2.4vw, 32px);
}

/* Landing: a floating pill */
.nav--landing {
  top: 14px;
  padding: 0 var(--gutter);
}
.nav--landing .nav-inner {
  max-width: 1120px;
  height: 56px;
  margin: 0 auto;
  padding: 0 8px 0 16px;
  border-radius: 999px;
  border: 1px solid transparent;
  transition:
    background-color 0.4s,
    border-color 0.4s,
    box-shadow 0.4s,
    max-width 0.6s var(--ease-out);
}
.nav--landing.scrolled .nav-inner,
.nav--landing.open .nav-inner {
  max-width: 980px;
  background: color-mix(in oklab, var(--bg-elev) 72%, transparent);
  border-color: var(--border);
  box-shadow: var(--shadow-md);
  backdrop-filter: blur(20px) saturate(1.5);
  -webkit-backdrop-filter: blur(20px) saturate(1.5);
}
.nav--landing.hidden {
  transform: translateY(-120%);
}

.brand {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-right: auto;
  font-weight: 640;
  font-size: 16.5px;
  letter-spacing: -0.025em;
}

.links {
  position: relative;
  display: flex;
  align-items: center;
  gap: 2px;
}
.hover-pill {
  position: absolute;
  left: 0;
  top: 0;
  height: 100%;
  border-radius: 999px;
  background: var(--surface-2);
  pointer-events: none;
  transition:
    transform 0.4s var(--ease-out),
    width 0.4s var(--ease-out),
    opacity 0.25s;
}
.link {
  position: relative;
  padding: 7px 14px;
  border-radius: 999px;
  font-size: 14.5px;
  font-weight: 500;
  color: var(--text-2);
  transition: color 0.2s;
}
.link:hover,
.link.active {
  color: var(--text);
}
.link.active::after {
  content: '';
  position: absolute;
  left: 50%;
  bottom: 1px;
  width: 4px;
  height: 4px;
  margin-left: -2px;
  border-radius: 50%;
  background: var(--accent);
  box-shadow: 0 0 10px var(--accent);
}

.actions {
  display: flex;
  align-items: center;
  gap: 4px;
  margin-left: auto;
}
.search-btn {
  display: flex;
  align-items: center;
  gap: 8px;
  height: 38px;
  padding: 0 8px 0 12px;
  margin-right: 4px;
  border-radius: 999px;
  border: 1px solid var(--border);
  background: var(--surface);
  color: var(--text-3);
  font-size: 14px;
  transition:
    border-color 0.2s,
    color 0.2s,
    background-color 0.2s;
}
.search-btn:hover {
  color: var(--text-2);
  border-color: var(--border-strong);
  background: var(--surface-2);
}
.search-label {
  padding-right: 24px;
}
.search-kbd {
  font-family: var(--font-mono);
  font-size: 11px;
  padding: 2px 7px;
  border-radius: 999px;
  border: 1px solid var(--border);
  color: var(--text-3);
}
.icon-btn {
  display: grid;
  place-items: center;
  width: 38px;
  height: 38px;
  border-radius: 999px;
  color: var(--text-2);
  transition:
    color 0.2s,
    background-color 0.2s;
}
.icon-btn:hover {
  color: var(--text);
  background: var(--surface-2);
}
.menu-btn {
  display: none;
}

@media (max-width: 860px) {
  .links,
  .search-label,
  .search-kbd,
  .gh {
    display: none;
  }
  .search-btn {
    width: 38px;
    padding: 0;
    justify-content: center;
    border: 0;
    background: none;
    margin: 0;
  }
  .menu-btn {
    display: grid;
  }
}

/* Mobile menu */
.mobile-menu {
  position: fixed;
  inset: 0;
  z-index: 45;
  display: flex;
  flex-direction: column;
  justify-content: space-between;
  padding: calc(var(--nav-h) + 40px) var(--gutter) 40px;
  background: color-mix(in oklab, var(--bg) 88%, transparent);
  backdrop-filter: blur(24px);
  -webkit-backdrop-filter: blur(24px);
}
.mobile-menu nav {
  display: grid;
  gap: 4px;
}
.m-link {
  display: flex;
  align-items: baseline;
  gap: 16px;
  padding: 10px 0;
  font-size: clamp(34px, 10vw, 52px);
  font-weight: 600;
  letter-spacing: -0.04em;
  line-height: 1.1;
  border-bottom: 1px solid var(--border);
}
.m-index {
  font-size: 12px;
  color: var(--accent);
  letter-spacing: 0.1em;
  font-weight: 500;
}
.m-link,
.m-foot {
  animation: m-in 0.6s var(--ease-out) both;
  animation-delay: calc(var(--i) * 55ms + 80ms);
}
@keyframes m-in {
  from {
    opacity: 0;
    transform: translateY(24px);
    filter: blur(6px);
  }
}
.menu-enter-active,
.menu-leave-active {
  transition:
    opacity 0.35s var(--ease-out),
    clip-path 0.5s var(--ease-out);
}
.menu-enter-from,
.menu-leave-to {
  opacity: 0;
  clip-path: circle(0% at calc(100% - 40px) 40px);
}
.menu-enter-to,
.menu-leave-from {
  clip-path: circle(150% at calc(100% - 40px) 40px);
}
</style>
