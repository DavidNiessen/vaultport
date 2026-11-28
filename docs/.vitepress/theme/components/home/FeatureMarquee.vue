<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script setup lang="ts">
/**
 * Two endless rows of capabilities scrolling in opposite directions. Scrolling the page speeds
 * them up and skews them a little, then they ease back.
 */
import { onBeforeUnmount, onMounted, ref } from 'vue';

import { prefersReducedMotion } from '../../composables/motion';

const ROW_A = [
  'SSH',
  'SFTP',
  'Mosh',
  'Telnet',
  'Serial',
  'Jump hosts',
  'SOCKS5 & HTTP proxies',
  'Port forwarding',
  'Agent forwarding',
  'OpenSSH certificates',
  'Post-quantum KEX',
];
const ROW_B = [
  'FIDO2 security keys',
  'Secure Enclave',
  'Windows Hello',
  'Android Keystore',
  'Split workspaces',
  'Broadcast input',
  'Snippets',
  'Session logs',
  'QR pairing',
  'Local AI',
  'MCP for AI agents',
];

const skew = ref(0);
const boost = ref(0);
let lastY = 0;
let raf = 0;
let velocity = 0;
function tick() {
  const y = window.scrollY;
  velocity = velocity * 0.88 + (y - lastY) * 0.12;
  lastY = y;
  skew.value = Math.max(-8, Math.min(8, velocity * 0.25));
  boost.value = Math.min(4, Math.abs(velocity) * 0.08);
  raf = requestAnimationFrame(tick);
}
onMounted(() => {
  if (prefersReducedMotion()) return;
  lastY = window.scrollY;
  raf = requestAnimationFrame(tick);
});
onBeforeUnmount(() => cancelAnimationFrame(raf));
</script>

<template>
  <section class="marquee" aria-label="Capabilities">
    <div
      v-for="(row, r) in [ROW_A, ROW_B]"
      :key="r"
      class="row"
      :class="{ reverse: r === 1 }"
      :style="{ transform: `skewX(${r === 1 ? -skew : skew}deg)` }"
    >
      <div class="track" :style="{ '--boost': boost }">
        <template v-for="copy in 2" :key="copy">
          <span v-for="item in row" :key="`${copy}-${item}`" class="item" :aria-hidden="copy === 2">
            <span class="star" aria-hidden="true">✦</span>{{ item }}
          </span>
        </template>
      </div>
    </div>
  </section>
</template>

<style scoped>
.marquee {
  display: grid;
  gap: 14px;
  padding: 96px 0 40px;
  overflow: hidden;
  mask-image: linear-gradient(90deg, transparent, #000 12%, #000 88%, transparent);
  -webkit-mask-image: linear-gradient(90deg, transparent, #000 12%, #000 88%, transparent);
}
.row {
  display: flex;
  transition: transform 0.2s linear;
}
.track {
  display: flex;
  flex: none;
  gap: 14px;
  padding-right: 14px;
  animation: scroll 60s linear infinite;
}
.reverse .track {
  animation-direction: reverse;
  animation-duration: 70s;
}
.row:hover .track {
  animation-play-state: paused;
}
@keyframes scroll {
  to {
    transform: translateX(-50%);
  }
}
.item {
  display: inline-flex;
  align-items: center;
  gap: 12px;
  padding: 12px 22px;
  border-radius: 999px;
  border: 1px solid var(--border);
  background: var(--surface);
  font-size: clamp(15px, 1.4vw, 19px);
  font-weight: 500;
  letter-spacing: -0.015em;
  white-space: nowrap;
  color: var(--text-2);
  transition:
    color 0.2s,
    border-color 0.2s,
    background-color 0.2s;
}
.item:hover {
  color: var(--text);
  border-color: color-mix(in oklab, var(--accent) 50%, transparent);
  background: var(--accent-soft);
}
.star {
  color: var(--accent);
  font-size: 0.8em;
}
</style>
