// SPDX-License-Identifier: GPL-3.0-or-later
/** Open state of the search palette, shared by the nav button, shortcuts and the palette. */
import { ref } from 'vue';

export const searchOpen = ref(false);

export function openSearch(): void {
  searchOpen.value = true;
}

/** "⌘" on Apple platforms, "Ctrl" elsewhere; `null` until detected after mount (stable SSR). */
export const modKey = ref<string | null>(null);

export function detectModKey(): void {
  modKey.value = /mac|iphone|ipad/i.test(navigator.platform || navigator.userAgent) ? '⌘' : 'Ctrl';
}
