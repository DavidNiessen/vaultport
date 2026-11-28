<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script setup lang="ts">
/** One quiet, slowly scrolling line of what Vaultport speaks and supports. */
const ITEMS = [
  'SSH',
  'SFTP',
  'Mosh',
  'Telnet',
  'Serial',
  'Local shells',
  'Jump hosts',
  'SOCKS5 & HTTP proxies',
  'Port forwarding',
  'Agent forwarding',
  'OpenSSH certificates',
  'Post-quantum key exchange',
  'FIDO2 security keys',
  'Secure Enclave',
  'Windows Hello',
  'Android Keystore',
  'Split workspaces',
  'Snippets',
  'Session logs',
  'Local AI',
  'MCP for AI agents',
];
</script>

<template>
  <section class="marquee" aria-label="Capabilities">
    <ul class="sr-only">
      <li v-for="item in ITEMS" :key="item">{{ item }}</li>
    </ul>
    <div class="track" aria-hidden="true">
      <template v-for="copy in 2" :key="copy">
        <span
          v-for="item in ITEMS"
          :key="`${copy}-${item}`"
          class="item"
          :class="{ dup: copy === 2 }"
          >{{ item }}</span
        >
      </template>
    </div>
  </section>
</template>

<style scoped>
.marquee {
  padding: 88px 0 24px;
  overflow: hidden;
  mask-image: linear-gradient(90deg, transparent, #000 10%, #000 90%, transparent);
  -webkit-mask-image: linear-gradient(90deg, transparent, #000 10%, #000 90%, transparent);
}
.track {
  display: flex;
  width: max-content;
  animation: scroll 90s linear infinite;
}
.marquee:hover .track {
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
  white-space: nowrap;
  font-size: 15px;
  color: var(--text-3);
  transition: color 0.2s;
}
.item:hover {
  color: var(--text);
}
.item::after {
  content: '';
  width: 3px;
  height: 3px;
  margin: 0 22px;
  border-radius: 50%;
  background: var(--border-strong);
}
@media (prefers-reduced-motion: reduce) {
  .track {
    animation: none;
    flex-wrap: wrap;
    width: auto;
    justify-content: center;
    row-gap: 8px;
  }
  .dup {
    display: none;
  }
}
</style>
