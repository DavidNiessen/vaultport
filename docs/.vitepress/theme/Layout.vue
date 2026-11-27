<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script setup lang="ts">
/** The default theme layout plus the site footer on marketing pages (home, download). */
import { useData } from 'vitepress';
import DefaultTheme from 'vitepress/theme-without-fonts';
import { computed } from 'vue';

import SiteFooter from './components/SiteFooter.vue';

const { frontmatter } = useData();
const showFooter = computed(() => frontmatter.value.layout === 'home');
</script>

<template>
  <DefaultTheme.Layout>
    <template #layout-bottom>
      <SiteFooter v-if="showFooter" />
    </template>
  </DefaultTheme.Layout>
</template>
