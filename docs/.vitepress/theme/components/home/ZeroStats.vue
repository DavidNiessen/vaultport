<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script setup lang="ts">
/** Numbers that count down to the only right answer: zero. */
import { onBeforeUnmount, onMounted, ref } from 'vue';

import { prefersReducedMotion } from '../../composables/motion';

const STATS = [
  { from: 1_284_390, label: 'Accounts you need to create', note: 'Install it and go.' },
  { from: 48_210, label: 'Servers we run', note: 'Nothing to breach, nothing to shut down.' },
  { from: 912, label: 'Trackers & analytics scripts', note: 'Not in the app, not on this site.' },
  {
    from: 7_340_112,
    label: 'Bytes of your data we can read',
    note: 'Sealed on your device, always.',
  },
];

const values = ref(STATS.map(() => 0));
const root = ref<HTMLElement | null>(null);
let raf = 0;
let observer: IntersectionObserver | null = null;
const format = (n: number) => new Intl.NumberFormat('en').format(Math.round(n));

function run() {
  if (prefersReducedMotion()) return;
  const start = performance.now();
  const step = (now: number) => {
    const t = Math.min(1, (now - start) / 2200);
    const eased = 1 - Math.pow(1 - t, 4);
    values.value = STATS.map((s) => s.from * (1 - eased));
    if (t < 1) raf = requestAnimationFrame(step);
  };
  values.value = STATS.map((s) => s.from);
  raf = requestAnimationFrame(step);
}

onMounted(() => {
  if (!root.value) return;
  observer = new IntersectionObserver(
    ([entry]) => {
      if (!entry?.isIntersecting) return;
      observer?.disconnect();
      run();
    },
    { threshold: 0.4 },
  );
  observer.observe(root.value);
});
onBeforeUnmount(() => {
  cancelAnimationFrame(raf);
  observer?.disconnect();
});
</script>

<template>
  <section ref="root" class="zero">
    <div class="container">
      <p class="eyebrow" v-reveal>By the numbers</p>
      <div class="grid">
        <div v-for="(s, i) in STATS" :key="s.label" v-reveal="i * 90" class="stat">
          <span class="num mono" :class="{ landed: values[i] === 0 }">{{
            format(values[i] ?? 0)
          }}</span>
          <span class="label">{{ s.label }}</span>
          <span class="note">{{ s.note }}</span>
        </div>
      </div>
    </div>
  </section>
</template>

<style scoped>
.zero {
  padding: 100px 0 40px;
}
.grid {
  display: grid;
  grid-template-columns: repeat(4, 1fr);
  margin-top: 28px;
  border-top: 1px solid var(--border);
}
.stat {
  display: grid;
  align-content: start;
  gap: 8px;
  padding: 28px 24px 28px 0;
  border-right: 1px solid var(--border);
}
.stat + .stat {
  padding-left: 24px;
}
.stat:last-child {
  border-right: 0;
}
.num {
  font-size: clamp(48px, 6vw, 88px);
  line-height: 1;
  font-weight: 500;
  letter-spacing: -0.06em;
  color: var(--text-3);
  font-variant-numeric: tabular-nums;
  transition:
    color 0.5s,
    text-shadow 0.5s;
}
.num.landed {
  color: var(--text);
  background: var(--grad-text);
  -webkit-background-clip: text;
  background-clip: text;
  -webkit-text-fill-color: transparent;
  filter: drop-shadow(0 0 30px rgb(var(--glow-1) / 0.35));
}
.label {
  font-weight: 600;
  font-size: 15px;
  letter-spacing: -0.01em;
}
.note {
  font-size: 13.5px;
  color: var(--text-3);
}
@media (max-width: 900px) {
  .grid {
    grid-template-columns: 1fr 1fr;
  }
  .stat:nth-child(2) {
    border-right: 0;
  }
  .stat:nth-child(3) {
    padding-left: 0;
  }
  .stat:nth-child(n + 3) {
    border-top: 1px solid var(--border);
  }
}
@media (max-width: 520px) {
  .grid {
    grid-template-columns: 1fr;
  }
  .stat,
  .stat + .stat {
    padding-left: 0;
    border-right: 0;
    border-top: 1px solid var(--border);
  }
}
</style>
