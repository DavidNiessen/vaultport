// SPDX-License-Identifier: GPL-3.0-or-later
/** Vaultport's own theme (no VitePress default theme): landing pages, docs and search. */
import type { Theme } from 'vitepress';

import '@fontsource-variable/geist';
import '@fontsource-variable/geist-mono';
import '@fontsource/instrument-serif/400.css';
import '@fontsource/instrument-serif/400-italic.css';
import './styles/tokens.css';
import './styles/base.css';
import './styles/prose.css';

import DownloadPage from './components/download/DownloadPage.vue';
import HomePage from './components/home/HomePage.vue';
import { vMagnetic, vReveal, vSpotlight, vTilt } from './composables/motion';
import Layout from './Layout.vue';

export default {
  Layout,
  enhanceApp({ app }) {
    app.component('HomePage', HomePage);
    app.component('DownloadPage', DownloadPage);
    app.directive('reveal', vReveal);
    app.directive('spotlight', vSpotlight);
    app.directive('magnetic', vMagnetic);
    app.directive('tilt', vTilt);
  },
} satisfies Theme;
