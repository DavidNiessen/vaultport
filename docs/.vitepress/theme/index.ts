// SPDX-License-Identifier: GPL-3.0-or-later
import type { Theme } from 'vitepress';
// The default theme's CSS must load before ours so our tokens override its variables.
import DefaultTheme from 'vitepress/theme-without-fonts';

import '@fontsource-variable/inter';
import '@fontsource/jetbrains-mono/400.css';
import '@fontsource/jetbrains-mono/500.css';
import '@fontsource/jetbrains-mono/600.css';
import './style.css';

import DownloadPage from './components/DownloadPage.vue';
import HomePage from './components/HomePage.vue';
import { vReveal } from './composables/reveal';
import Layout from './Layout.vue';

export default {
  extends: DefaultTheme,
  Layout,
  enhanceApp({ app }) {
    app.component('HomePage', HomePage);
    app.component('DownloadPage', DownloadPage);
    app.directive('reveal', vReveal);
  },
} satisfies Theme;
