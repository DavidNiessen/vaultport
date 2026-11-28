<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script setup lang="ts">
/** Security principles as big editorial rows; hovering one wipes in colour and opens it up. */
import { ArrowUpRight } from '@lucide/vue';
import { withBase } from 'vitepress';

const ROWS = [
  {
    title: 'Secrets never reach the UI',
    text: 'Keys and passwords live only in the Rust core, which acts on opaque ids and zeroises memory after use.',
    link: '/security/app-isolation',
  },
  {
    title: 'Strict about host keys',
    text: 'Unknown and changed keys stop the connection, with @cert-authority and @revoked support built in.',
    link: '/security/host-keys',
  },
  {
    title: 'Sync that proves itself',
    text: 'Every change is sealed and hash-chained, so storage can neither read it nor quietly roll it back.',
    link: '/security/sync',
  },
  {
    title: 'A supply chain you can check',
    text: 'Actions pinned by SHA, cargo-deny, CodeQL, nightly fuzzing; signed checksums, SBOM and provenance per release.',
    link: '/security/supply-chain',
  },
  {
    title: 'Open, all of it',
    text: 'GPL-3.0-or-later from the core to the build pipeline. Read it, build it, verify your download.',
    link: '/development/building',
  },
];
</script>

<template>
  <section class="trust">
    <div class="container">
      <div class="head">
        <p class="eyebrow" v-reveal>Security model</p>
        <h2 class="h-section" v-reveal="80">
          Trust is earned <span class="serif grad-text">in the open.</span>
        </h2>
      </div>
      <ol class="rows">
        <li v-for="(r, i) in ROWS" :key="r.title" v-reveal="i * 60">
          <a :href="withBase(r.link)" class="row">
            <span class="idx mono">0{{ i + 1 }}</span>
            <span class="title">{{ r.title }}</span>
            <span class="text">{{ r.text }}</span>
            <span class="go"><ArrowUpRight :size="22" /></span>
          </a>
        </li>
      </ol>
      <a :href="withBase('/security/')" class="btn" v-reveal>Read the threat model</a>
    </div>
  </section>
</template>

<style scoped>
.trust {
  padding: 120px 0 60px;
}
.head {
  display: grid;
  gap: 18px;
  margin-bottom: 40px;
}
.rows {
  list-style: none;
  padding: 0;
  margin: 0 0 36px;
  border-top: 1px solid var(--border);
}
.row {
  position: relative;
  display: grid;
  grid-template-columns: 60px minmax(0, 1.1fr) minmax(0, 1fr) 40px;
  align-items: center;
  gap: 24px;
  padding: 30px 12px;
  border-bottom: 1px solid var(--border);
  isolation: isolate;
  overflow: hidden;
}
.row::before {
  content: '';
  position: absolute;
  inset: 0;
  z-index: -1;
  background: linear-gradient(
    90deg,
    rgb(var(--glow-1) / 0.16),
    rgb(var(--glow-2) / 0.06) 60%,
    transparent
  );
  transform: scaleX(0);
  transform-origin: left;
  transition: transform 0.7s var(--ease-out);
}
.row:hover::before,
.row:focus-visible::before {
  transform: scaleX(1);
}
.idx {
  font-size: 13px;
  color: var(--text-3);
  transition: color 0.3s;
}
.title {
  font-size: clamp(22px, 2.6vw, 34px);
  font-weight: 600;
  letter-spacing: -0.035em;
  line-height: 1.1;
  transition: transform 0.5s var(--ease-out);
}
.text {
  font-size: 15px;
  color: var(--text-2);
  line-height: 1.55;
  opacity: 0.6;
  transition: opacity 0.4s;
}
.go {
  display: grid;
  place-items: center;
  width: 40px;
  height: 40px;
  border-radius: 50%;
  border: 1px solid var(--border);
  color: var(--text-2);
  transition:
    transform 0.5s var(--ease-out),
    background-color 0.3s,
    color 0.3s,
    border-color 0.3s;
}
.row:hover .idx {
  color: var(--accent);
}
.row:hover .title {
  transform: translateX(10px);
}
.row:hover .text {
  opacity: 1;
}
.row:hover .go {
  transform: rotate(45deg);
  background: var(--text);
  border-color: var(--text);
  color: var(--bg);
}
@media (max-width: 860px) {
  .row {
    grid-template-columns: 40px 1fr 40px;
    gap: 8px 16px;
    padding: 22px 4px;
  }
  .text {
    grid-column: 2;
    grid-row: 2;
    opacity: 1;
  }
  .go {
    grid-column: 3;
    grid-row: 1 / span 2;
  }
}
</style>
