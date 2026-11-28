// SPDX-License-Identifier: GPL-3.0-or-later
/// <reference types="vitepress/client" />

import type { vMagnetic, vReveal, vSpotlight, vTilt } from './theme/composables/motion';

declare module 'vue' {
  interface GlobalDirectives {
    vReveal: typeof vReveal;
    vSpotlight: typeof vSpotlight;
    vMagnetic: typeof vMagnetic;
    vTilt: typeof vTilt;
  }
}

declare module '*.vue' {
  import type { DefineComponent } from 'vue';
  const component: DefineComponent;
  export default component;
}
