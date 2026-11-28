<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script setup lang="ts">
/** Picks the page layout: landing (full-bleed marketing pages), docs, or not found. */
import { useData, useRoute } from 'vitepress';
import { computed } from 'vue';

import NotFound from './components/chrome/NotFound.vue';
import ReadingProgress from './components/chrome/ReadingProgress.vue';
import SearchPalette from './components/chrome/SearchPalette.vue';
import SiteFooter from './components/chrome/SiteFooter.vue';
import SiteNav from './components/chrome/SiteNav.vue';
import DocLayout from './components/docs/DocLayout.vue';

const { page, frontmatter } = useData();
const route = useRoute();

const layout = computed(() => {
  if (page.value.isNotFound) return 'notfound' as const;
  return frontmatter.value.layout === 'landing' ? ('landing' as const) : ('doc' as const);
});
</script>

<template>
  <div class="site" :class="`site--${layout}`">
    <a class="skip-link" href="#main">Skip to content</a>
    <SiteNav :variant="layout" />
    <ReadingProgress v-if="layout === 'doc'" />
    <SearchPalette />
    <main id="main">
      <NotFound v-if="layout === 'notfound'" />
      <div v-else-if="layout === 'landing'" :key="route.path" class="page-enter">
        <Content />
      </div>
      <DocLayout v-else />
    </main>
    <SiteFooter v-if="layout !== 'doc'" />
    <div class="grain" aria-hidden="true" />
  </div>
</template>
