<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script setup lang="ts">
import { ArrowRight, Download } from '@lucide/vue';
import { withBase } from 'vitepress';
import { computed } from 'vue';

import { REPO_URL } from '../../site';
import { OS_LABEL, usePlatform } from '../composables/platform';
import AppWindow from './AppWindow.vue';
import GithubMark from './GithubMark.vue';

const platform = usePlatform();

const download = computed(() => {
  const os = platform.value?.os;
  return {
    label: os ? `Download for ${OS_LABEL[os]}` : 'Download',
    href: withBase(os ? `/download#${os}` : '/download'),
  };
});
</script>

<template>
  <section class="hero" aria-labelledby="hero-title">
    <div class="backdrop" aria-hidden="true">
      <div class="grid" />
      <div class="orb orb-a" />
      <div class="orb orb-b" />
    </div>

    <div class="inner">
      <a class="eyebrow" :href="withBase('/security/')">
        <span class="pulse" aria-hidden="true" />
        Post-quantum key exchange · No servers of ours
        <ArrowRight :size="14" aria-hidden="true" />
      </a>

      <h1 id="hero-title" class="title">
        The SSH client that <span class="gradient line">keeps your keys yours.</span>
      </h1>

      <p class="lead">
        SSH, SFTP, Mosh, Telnet and serial in one fast app for desktop and mobile. Hosts, keys and
        passwords live in an end-to-end encrypted vault that syncs only through storage you own — no
        account, no cloud, no telemetry.
      </p>

      <div class="actions">
        <a class="btn primary" :href="download.href">
          <Download :size="18" aria-hidden="true" />
          <span>{{ download.label }}</span>
        </a>
        <a class="btn secondary" :href="REPO_URL" target="_blank" rel="noopener">
          <GithubMark :size="18" />
          <span>View on GitHub</span>
        </a>
      </div>

      <p class="meta">
        Free &amp; open source (GPL-3.0) · Windows · macOS · Linux · iOS &amp; iPadOS · Android
      </p>

      <div class="stage">
        <div class="glow" aria-hidden="true" />
        <AppWindow class="app" />
      </div>
    </div>
  </section>
</template>

<style scoped>
.hero {
  position: relative;
  isolation: isolate;
  padding: 72px 24px 48px;
}

.backdrop {
  position: absolute;
  inset: 0;
  z-index: -1;
  pointer-events: none;
}

.grid {
  position: absolute;
  inset: 0;
  background-image:
    linear-gradient(var(--sc-grid-line) 1px, transparent 1px),
    linear-gradient(90deg, var(--sc-grid-line) 1px, transparent 1px);
  background-size: 56px 56px;
  mask-image: radial-gradient(ellipse 70% 55% at 50% 30%, #000 30%, transparent 75%);
}

.orb {
  position: absolute;
  border-radius: 50%;
  filter: blur(80px);
  opacity: 0.55;
  animation: drift 18s ease-in-out infinite alternate;
}

.orb-a {
  top: -160px;
  left: 50%;
  width: 720px;
  height: 480px;
  margin-left: -520px;
  background: radial-gradient(circle, rgb(99 102 241 / 0.55), transparent 70%);
}

.orb-b {
  top: -60px;
  left: 50%;
  width: 560px;
  height: 420px;
  margin-left: 40px;
  background: radial-gradient(circle, rgb(34 211 238 / 0.32), transparent 70%);
  animation-duration: 22s;
  animation-direction: alternate-reverse;
}

:root:not(.dark) .orb {
  opacity: 0.35;
}

@keyframes drift {
  from {
    transform: translate3d(-30px, 0, 0) scale(1);
  }
  to {
    transform: translate3d(40px, 30px, 0) scale(1.08);
  }
}

.inner {
  max-width: 1152px;
  margin: 0 auto;
  text-align: center;
}

.eyebrow {
  display: inline-flex;
  align-items: center;
  gap: 10px;
  padding: 6px 14px 6px 12px;
  border: 1px solid var(--sc-border-strong);
  border-radius: 999px;
  background: color-mix(in srgb, var(--vp-c-bg) 70%, transparent);
  color: var(--vp-c-text-2);
  font-size: 13px;
  font-weight: 500;
  text-decoration: none;
  backdrop-filter: blur(8px);
  transition:
    border-color 0.2s,
    color 0.2s;
}

.eyebrow:hover {
  border-color: var(--vp-c-brand-2);
  color: var(--vp-c-text-1);
}

.pulse {
  position: relative;
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: var(--sc-success);
}

.pulse::after {
  content: '';
  position: absolute;
  inset: 0;
  border-radius: 50%;
  background: var(--sc-success);
  animation: ping 2.4s cubic-bezier(0, 0, 0.2, 1) infinite;
}

@keyframes ping {
  75%,
  100% {
    transform: scale(2.6);
    opacity: 0;
  }
}

.title {
  margin: 28px auto 0;
  max-width: 900px;
  color: var(--vp-c-text-1);
  font-size: clamp(40px, 7vw, 76px);
  font-weight: 700;
  line-height: 1.04;
  letter-spacing: -0.035em;
  text-wrap: balance;
}

.line {
  display: block;
}

.gradient {
  background: linear-gradient(100deg, #818cf8 0%, #6366f1 35%, #22d3ee 100%);
  background-clip: text;
  -webkit-background-clip: text;
  color: transparent;
}

:root:not(.dark) .gradient {
  background-image: linear-gradient(100deg, #4f46e5 0%, #6366f1 40%, #0891b2 100%);
}

.lead {
  max-width: 680px;
  margin: 24px auto 0;
  color: var(--vp-c-text-2);
  font-size: clamp(16px, 2vw, 19px);
  line-height: 1.6;
  text-wrap: pretty;
}

.actions {
  display: flex;
  flex-wrap: wrap;
  justify-content: center;
  gap: 12px;
  margin-top: 36px;
}

.btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 10px;
  height: 48px;
  padding: 0 22px;
  border-radius: 12px;
  font-size: 15px;
  font-weight: 600;
  text-decoration: none;
  transition:
    transform 0.2s var(--sc-ease),
    box-shadow 0.2s var(--sc-ease),
    background-color 0.2s;
}

.btn:hover {
  transform: translateY(-1px);
}

.btn.primary {
  background: linear-gradient(180deg, #7477f5, #6366f1 60%, #5b5ef0);
  color: #fff;
  box-shadow:
    0 0 0 1px rgb(99 102 241 / 0.6),
    0 1px 0 0 rgb(255 255 255 / 0.25) inset,
    0 12px 32px -10px rgb(99 102 241 / 0.75);
}

.btn.primary:hover {
  box-shadow:
    0 0 0 1px rgb(129 140 248 / 0.9),
    0 1px 0 0 rgb(255 255 255 / 0.3) inset,
    0 16px 40px -10px rgb(99 102 241 / 0.9);
}

.btn.secondary {
  background: var(--vp-c-bg-soft);
  color: var(--vp-c-text-1);
  box-shadow: 0 0 0 1px var(--sc-border-strong);
}

.btn.secondary:hover {
  background: var(--vp-c-bg-elv);
}

.meta {
  margin: 18px 0 0;
  color: var(--vp-c-text-3);
  font-size: 13px;
}

.stage {
  position: relative;
  max-width: 1040px;
  margin: 64px auto 0;
  perspective: 1600px;
}

.glow {
  position: absolute;
  inset: 10% 8% -6%;
  z-index: -1;
  border-radius: 40px;
  background: radial-gradient(60% 60% at 50% 40%, var(--sc-glow), transparent 70%);
  filter: blur(40px);
}

.app {
  transform-origin: 50% 0;
  animation: rise 1.2s var(--sc-ease) both 0.15s;
}

@keyframes rise {
  from {
    opacity: 0;
    transform: rotateX(18deg) translateY(40px) scale(0.96);
  }
  to {
    opacity: 1;
    transform: none;
  }
}

@media (max-width: 640px) {
  .hero {
    padding: 48px 16px 24px;
  }
  .title .line {
    display: inline;
  }
  .actions {
    flex-direction: column;
    align-items: stretch;
  }
  .stage {
    margin-top: 44px;
  }
  .eyebrow {
    font-size: 12px;
  }
}

@media (prefers-reduced-motion: reduce) {
  .orb,
  .pulse::after,
  .app {
    animation: none;
  }
  .btn {
    transition: none;
  }
}
</style>
