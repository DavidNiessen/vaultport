<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script setup lang="ts">
/** A thin gradient bar under the nav that fills as the page is read. */
import { onBeforeUnmount, onMounted, ref } from 'vue';

const progress = ref(0);
let frame = 0;
function update() {
  frame = 0;
  const max = document.documentElement.scrollHeight - window.innerHeight;
  progress.value = max > 0 ? Math.min(1, window.scrollY / max) : 0;
}
function onScroll() {
  if (!frame) frame = requestAnimationFrame(update);
}
onMounted(() => {
  update();
  window.addEventListener('scroll', onScroll, { passive: true });
  window.addEventListener('resize', onScroll, { passive: true });
});
onBeforeUnmount(() => {
  window.removeEventListener('scroll', onScroll);
  window.removeEventListener('resize', onScroll);
});
</script>

<template>
  <div class="progress" aria-hidden="true">
    <span :style="{ transform: `scaleX(${progress})` }" />
  </div>
</template>

<style scoped>
.progress {
  position: fixed;
  inset: var(--nav-h) 0 auto;
  z-index: 51;
  height: 2px;
  pointer-events: none;
}
span {
  display: block;
  height: 100%;
  transform-origin: left;
  background: linear-gradient(90deg, var(--accent), var(--accent-2), rgb(var(--glow-3)));
  box-shadow: 0 0 12px rgb(var(--glow-1) / 0.8);
}
</style>
