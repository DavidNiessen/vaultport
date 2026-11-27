<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script setup lang="ts">
import { ArrowDown, ArrowRight } from '@lucide/vue';
import { withBase } from 'vitepress';
import { computed } from 'vue';

import { REPO_URL } from '../../site';
import { OS_LABEL, usePlatform } from '../composables/platform';
import AppWindow from './AppWindow.vue';
import LivingGlow, { type GlowBlob } from './LivingGlow.vue';
import GithubMark from './GithubMark.vue';

const platform = usePlatform();

const download = computed(() => {
  const os = platform.value?.os;
  return {
    label: os ? `Download for ${OS_LABEL[os]}` : 'Download free',
    href: withBase(os ? `/download#${os}` : '/download'),
  };
});

// Lights behind the hero: two big ones in the brand colours and a smaller one between them.
const glow: GlowBlob[] = [
  {
    box: { top: '-10%', left: '-12%', width: '62%', height: '80%' },
    tone: 1,
    strength: 1.5,
    wander: 29,
    morph: 22,
    path: 1,
    delay: -5,
  },
  {
    box: { top: '0%', right: '-14%', width: '52%', height: '75%' },
    tone: 2,
    strength: 1.25,
    wander: 35,
    morph: 27,
    path: 2,
    delay: -12,
  },
  {
    box: { top: '20%', left: '30%', width: '36%', height: '55%' },
    tone: 1,
    strength: 0.6,
    wander: 41,
    morph: 18,
    path: 3,
    delay: -20,
  },
];

const platforms = ['Windows', 'macOS', 'Linux', 'Android', 'iOS & iPadOS'];
</script>

<template>
  <section class="hero" aria-labelledby="hero-title">
    <LivingGlow class="aura" :blobs="glow" />

    <div class="inner">
      <a class="eyebrow" :href="withBase('/security/')">
        <span class="dot" aria-hidden="true" />
        Free &amp; open source · Post-quantum SSH
        <ArrowRight :size="14" aria-hidden="true" />
      </a>

      <h1 id="hero-title" class="title sc-display">
        Every server.
        <span class="accent">No cloud.</span>
      </h1>

      <p class="lead">
        A secure, serverless SSH, SFTP, Mosh, Telnet and serial client for every device. Your hosts
        and keys live in an end-to-end encrypted vault that syncs only through storage you own — no
        account, no telemetry.
      </p>

      <div class="actions">
        <a class="sc-pill primary" :href="download.href">
          <span>{{ download.label }}</span>
          <ArrowDown :size="18" aria-hidden="true" />
        </a>
        <a class="sc-pill ghost" :href="REPO_URL" target="_blank" rel="noopener">
          <GithubMark :size="18" />
          <span>View on GitHub</span>
        </a>
      </div>

      <ul class="platforms" aria-label="Available for">
        <li v-for="p in platforms" :key="p">{{ p }}</li>
      </ul>

      <div class="stage">
        <AppWindow class="app" />
      </div>
    </div>
  </section>
</template>

<style scoped>
.hero {
  position: relative;
  isolation: isolate;
  padding: 88px 24px 40px;
}

/* A soft two-tone glow from the colour preset, no motion. */

.aura {
  inset: -64px 0 25% 0;
  mask-image: linear-gradient(#000 55%, transparent);
}

.inner {
  max-width: 1152px;
  margin: 0 auto;
}

.eyebrow {
  display: inline-flex;
  align-items: center;
  gap: 10px;
  height: 34px;
  padding: 0 14px;
  border-radius: 999px;
  background: var(--sc-chip);
  color: var(--vp-c-text-1);
  font-size: 13px;
  font-weight: 700;
  text-decoration: none;
  backdrop-filter: blur(10px);
  transition: background-color 0.2s;
}

.eyebrow:hover {
  background: color-mix(in srgb, var(--vp-c-text-1) 12%, transparent);
}

.dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: var(--sc-accent);
}

.title {
  margin: 28px 0 0;
  color: var(--vp-c-text-1);
  font-size: clamp(44px, 8.4vw, 104px);
}

.accent {
  display: block;
  color: var(--sc-accent-text);
}

.lead {
  max-width: 660px;
  margin: 28px 0 0;
  color: var(--vp-c-text-2);
  font-size: clamp(17px, 2vw, 20px);
  font-weight: 500;
  line-height: 1.55;
  text-wrap: pretty;
}

.actions {
  display: flex;
  flex-wrap: wrap;
  gap: 12px;
  margin-top: 34px;
}

.platforms {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  margin: 24px 0 0;
  padding: 0;
  list-style: none;
}

.platforms li {
  display: inline-flex;
  align-items: center;
  height: 32px;
  padding: 0 14px;
  border: 1px solid var(--sc-border-strong);
  border-radius: 999px;
  color: var(--vp-c-text-2);
  font-size: 13px;
  font-weight: 700;
}

.stage {
  position: relative;
  margin-top: 72px;
}

.app {
  position: relative;
  box-shadow:
    var(--sc-shadow),
    0 0 120px -30px rgb(var(--sc-accent-rgb) / 0.35);
}

@media (max-width: 640px) {
  .hero {
    padding: 56px 20px 24px;
  }
  .stage {
    margin-top: 48px;
  }
}
</style>
