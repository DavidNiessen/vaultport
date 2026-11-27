<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script setup lang="ts">
/** A numbered row of capabilities under the hero. Scrolls slowly; static with reduced motion. */
const chips = [
  'SSH with post-quantum KEX',
  'Dual-pane SFTP',
  'Native Mosh',
  'Jump-host chains',
  'Port forwarding',
  'Serial & Telnet',
  'FIDO2 & Secure Enclave keys',
  'Split workspaces',
  'Encrypted session logs',
  'Local AI, off by default',
];
const pad = (n: number) => String(n + 1).padStart(2, '0');
</script>

<template>
  <section class="marquee" aria-label="Highlights">
    <ul class="track">
      <li v-for="(c, i) in chips" :key="c">
        <b>{{ pad(i) }}</b>{{ c }}
      </li>
    </ul>
    <ul class="track" aria-hidden="true">
      <li v-for="(c, i) in chips" :key="c">
        <b>{{ pad(i) }}</b>{{ c }}
      </li>
    </ul>
  </section>
</template>

<style scoped>
.marquee {
  display: flex;
  overflow: hidden;
  margin-top: 56px;
  padding: 22px 0;
  border-block: 1px solid var(--vp-c-divider);
  mask-image: linear-gradient(90deg, transparent, #000 6%, #000 94%, transparent);
}

.track {
  display: flex;
  flex-shrink: 0;
  gap: 12px;
  margin: 0;
  padding: 0 6px;
  list-style: none;
  animation: scroll 70s linear infinite;
}

.track li {
  display: inline-flex;
  align-items: center;
  gap: 12px;
  height: 46px;
  padding: 0 22px;
  border-radius: 999px;
  background: var(--sc-surface);
  box-shadow: 0 0 0 1px var(--sc-border);
  color: var(--vp-c-text-1);
  font-size: 15px;
  font-weight: 800;
  white-space: nowrap;
}

.track b {
  color: var(--sc-accent-text);
  font-family: var(--sc-font-display);
  font-size: 12px;
  font-stretch: 125%;
}

.marquee:hover .track {
  animation-play-state: paused;
}

@keyframes scroll {
  to {
    transform: translateX(-100%);
  }
}

@media (prefers-reduced-motion: reduce) {
  .track {
    flex-wrap: wrap;
    flex-shrink: 1;
    justify-content: center;
    row-gap: 12px;
    animation: none;
  }
  .track[aria-hidden='true'] {
    display: none;
  }
}
</style>
