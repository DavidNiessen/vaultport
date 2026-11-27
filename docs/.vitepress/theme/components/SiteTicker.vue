<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script setup lang="ts">
/** A slim strip of promises at the top of the home page. Scrolls slowly; static with reduced motion. */
const items = [
  'Free & open source · GPL-3.0',
  'No accounts',
  'No telemetry',
  'End-to-end encrypted sync',
  'Post-quantum key exchange',
  'Runs on every device',
];
</script>

<template>
  <div class="ticker">
    <ul class="track">
      <li v-for="i in items" :key="i">{{ i }}</li>
    </ul>
    <ul class="track" aria-hidden="true">
      <li v-for="i in items" :key="i">{{ i }}</li>
    </ul>
  </div>
</template>

<style scoped>
.ticker {
  display: flex;
  overflow: hidden;
  border-bottom: 1px solid rgb(var(--sc-accent-rgb) / 0.16);
  background: rgb(var(--sc-accent-rgb) / 0.07);
  color: var(--sc-accent-text);
  font-size: 12.5px;
  font-weight: 800;
  letter-spacing: 0.08em;
  text-transform: uppercase;
  white-space: nowrap;
}

.track {
  display: flex;
  flex-shrink: 0;
  margin: 0;
  padding: 0;
  list-style: none;
  animation: scroll 60s linear infinite;
}

.track li {
  display: flex;
  align-items: center;
  height: 36px;
  padding: 0 22px;
}

.track li::after {
  content: '✦';
  margin-left: 44px;
  opacity: 0.6;
}

.ticker:hover .track {
  animation-play-state: paused;
}

@keyframes scroll {
  to {
    transform: translateX(-100%);
  }
}

@media (prefers-reduced-motion: reduce) {
  .track {
    animation: none;
  }
}
</style>
