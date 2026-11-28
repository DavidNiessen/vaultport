<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script setup lang="ts">
/**
 * Footer for marketing pages: link columns and a giant outlined wordmark that fills with a
 * gradient where the pointer is.
 */
import { withBase } from 'vitepress';
import { ref } from 'vue';

import { LICENSE_URL, RELEASES_URL, REPO_URL, SECURITY_URL } from '../../../site';
import LogoMark from './LogoMark.vue';

const COLUMNS = [
  {
    title: 'Product',
    links: [
      { text: 'Download', href: withBase('/download') },
      { text: 'Releases', href: RELEASES_URL, external: true },
      { text: 'Phones & tablets', href: withBase('/guide/mobile') },
      { text: 'AI agents (MCP)', href: withBase('/guide/ai-agents') },
    ],
  },
  {
    title: 'Docs',
    links: [
      { text: 'Introduction', href: withBase('/guide/') },
      { text: 'Quick start', href: withBase('/guide/quick-start') },
      { text: 'FAQ', href: withBase('/faq') },
      { text: 'llms.txt', href: withBase('/llms.txt'), external: true },
    ],
  },
  {
    title: 'Security',
    links: [
      { text: 'Threat model', href: withBase('/security/') },
      { text: 'Vault cryptography', href: withBase('/security/vault-crypto') },
      { text: 'Supply chain', href: withBase('/security/supply-chain') },
      { text: 'Security policy', href: SECURITY_URL, external: true },
    ],
  },
  {
    title: 'Project',
    links: [
      { text: 'Source code', href: REPO_URL, external: true },
      { text: 'Contributing', href: withBase('/development/contributing') },
      { text: 'Architecture', href: withBase('/architecture') },
      { text: 'License (GPL-3.0)', href: LICENSE_URL, external: true },
    ],
  },
];

const mark = ref<HTMLElement | null>(null);
function move(e: PointerEvent) {
  const el = mark.value;
  if (!el) return;
  const r = el.getBoundingClientRect();
  el.style.setProperty('--fx', `${((e.clientX - r.left) / r.width) * 100}%`);
  el.style.setProperty('--fy', `${((e.clientY - r.top) / r.height) * 100}%`);
}
</script>

<template>
  <footer class="footer">
    <div class="container">
      <div class="top">
        <div class="about">
          <a :href="withBase('/')" class="brand"><LogoMark :size="32" /> Vaultport</a>
          <p>
            Secure, serverless SSH for every device. No account, no telemetry, no servers of ours.
          </p>
          <span class="status"><span class="dot" />All systems are yours</span>
        </div>
        <nav v-for="col in COLUMNS" :key="col.title" class="col" :aria-label="col.title">
          <p class="col-title">{{ col.title }}</p>
          <a
            v-for="l in col.links"
            :key="l.text"
            :href="l.href"
            :target="l.external ? '_blank' : undefined"
            :rel="l.external ? 'noreferrer' : undefined"
            >{{ l.text }}</a
          >
        </nav>
      </div>
    </div>
    <div ref="mark" class="wordmark" aria-hidden="true" @pointermove="move">VAULTPORT</div>
    <div class="bottom container">
      <span>© {{ new Date().getFullYear() }} Vaultport contributors · GPL-3.0-or-later</span>
      <span>Built in the open. This site sets no cookies and runs no analytics.</span>
    </div>
  </footer>
</template>

<style scoped>
.footer {
  position: relative;
  margin-top: 80px;
  padding-top: 72px;
  border-top: 1px solid var(--border);
  overflow: hidden;
  background: radial-gradient(60% 50% at 50% 100%, rgb(var(--glow-1) / 0.14), transparent 70%);
}
.top {
  display: grid;
  grid-template-columns: 1.6fr repeat(4, 1fr);
  gap: 32px;
}
.brand {
  display: inline-flex;
  align-items: center;
  gap: 10px;
  font-weight: 640;
  font-size: 18px;
  letter-spacing: -0.03em;
}
.about p {
  margin: 14px 0 18px;
  max-width: 300px;
  font-size: 14.5px;
  color: var(--text-2);
}
.status {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  padding: 6px 12px;
  border-radius: 999px;
  border: 1px solid var(--border);
  font-size: 12.5px;
  color: var(--text-2);
}
.dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: var(--ok);
  box-shadow: 0 0 0 0 var(--ok);
  animation: ping 2s infinite;
}
@keyframes ping {
  70% {
    box-shadow: 0 0 0 8px transparent;
  }
}
.col {
  display: grid;
  align-content: start;
  gap: 10px;
  font-size: 14.5px;
}
.col-title {
  font-size: 12px;
  font-weight: 600;
  letter-spacing: 0.08em;
  text-transform: uppercase;
  color: var(--text-3);
  margin-bottom: 4px;
}
.col a {
  padding: 3px 0;
  color: var(--text-2);
  width: fit-content;
  background: linear-gradient(currentColor, currentColor) 0 100% / 0 1px no-repeat;
  transition:
    color 0.2s,
    background-size 0.35s var(--ease-out);
}
.col a:hover {
  color: var(--text);
  background-size: 100% 1px;
}
.wordmark {
  --fx: 50%;
  --fy: 50%;
  margin-top: 64px;
  font-size: clamp(72px, 19vw, 300px);
  font-weight: 800;
  letter-spacing: -0.06em;
  line-height: 0.8;
  text-align: center;
  user-select: none;
  color: transparent;
  -webkit-text-stroke: 1px var(--border-strong);
  background: radial-gradient(
    circle at var(--fx) var(--fy),
    rgb(var(--glow-1)) 0%,
    rgb(var(--glow-2) / 0.8) 12%,
    transparent 30%
  );
  -webkit-background-clip: text;
  background-clip: text;
  transform: translateY(14%);
}
.bottom {
  position: relative;
  display: flex;
  flex-wrap: wrap;
  justify-content: space-between;
  gap: 8px;
  padding-block: 20px;
  font-size: 12.5px;
  color: var(--text-3);
  border-top: 1px solid var(--border);
  background: var(--bg);
}
@media (max-width: 900px) {
  .top {
    grid-template-columns: repeat(2, 1fr);
  }
  .about {
    grid-column: 1 / -1;
  }
}
</style>
