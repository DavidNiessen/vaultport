// SPDX-License-Identifier: GPL-3.0-or-later
/// <reference types="vitepress/client" />

import type { vReveal } from './theme/composables/reveal';

declare module 'vue' {
  interface GlobalDirectives {
    vReveal: typeof vReveal;
  }
}

declare module '*.vue' {
  import type { DefineComponent } from 'vue';
  const component: DefineComponent;
  export default component;
}
