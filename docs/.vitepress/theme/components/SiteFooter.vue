<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script setup lang="ts">
import { withBase } from 'vitepress';

import { LICENSE_URL, RELEASES_URL, REPO_URL, SECURITY_URL } from '../../site';

const columns = [
  {
    title: 'Product',
    links: [
      { text: 'Download', href: withBase('/download') },
      { text: 'Features', href: withBase('/#features') },
      { text: 'Security', href: withBase('/security/') },
      { text: 'FAQ', href: withBase('/faq') },
    ],
  },
  {
    title: 'Docs',
    links: [
      { text: 'Getting started', href: withBase('/guide/') },
      { text: 'Guides', href: withBase('/guide/hosts-and-groups') },
      { text: 'Architecture', href: withBase('/architecture') },
      { text: 'llms.txt', href: withBase('/llms.txt') },
    ],
  },
  {
    title: 'Project',
    links: [
      { text: 'Source code', href: REPO_URL, external: true },
      { text: 'Releases', href: RELEASES_URL, external: true },
      { text: 'Security policy', href: SECURITY_URL, external: true },
      { text: 'Contributing', href: withBase('/development/contributing') },
    ],
  },
];
</script>

<template>
  <footer class="footer">
    <div class="inner">
      <div class="brand">
        <a class="name" :href="withBase('/')">
          <img :src="withBase('/logo.svg')" alt="" width="28" height="28" />
          Vaultport
        </a>
        <p class="tag">A secure, serverless SSH client. No account, no cloud, no telemetry.</p>
      </div>
      <nav v-for="col in columns" :key="col.title" class="col" :aria-label="col.title">
        <h2 class="col-title">{{ col.title }}</h2>
        <ul>
          <li v-for="l in col.links" :key="l.text">
            <a
              :href="l.href"
              :target="'external' in l ? '_blank' : undefined"
              :rel="'external' in l ? 'noopener' : undefined"
              >{{ l.text }}</a
            >
          </li>
        </ul>
      </nav>
    </div>
    <div class="legal">
      <p>
        © 2026 David Niessen and contributors. Released under the
        <a :href="LICENSE_URL" target="_blank" rel="noopener">GNU GPL v3.0 or later</a>.
      </p>
      <p>This site uses no cookies, trackers or analytics.</p>
    </div>
  </footer>
</template>

<style scoped>
.footer {
  border-top: 1px solid var(--vp-c-divider);
  background: var(--vp-c-bg-alt);
}

.inner {
  display: grid;
  grid-template-columns: 2fr repeat(3, 1fr);
  gap: 32px;
  max-width: 1152px;
  margin: 0 auto;
  padding: 56px 24px 32px;
}

.name {
  display: inline-flex;
  align-items: center;
  gap: 10px;
  color: var(--vp-c-text-1);
  font-size: 17px;
  font-weight: 700;
  text-decoration: none;
}

.tag {
  max-width: 320px;
  margin: 12px 0 0;
  color: var(--vp-c-text-2);
  font-size: 14px;
  line-height: 1.6;
}

.col-title {
  margin: 0 0 12px;
  color: var(--vp-c-text-1);
  font-size: 13px;
  font-weight: 600;
}

.col ul {
  display: grid;
  gap: 8px;
  margin: 0;
  padding: 0;
  list-style: none;
}

.col a,
.legal a {
  color: var(--vp-c-text-2);
  font-size: 14px;
  text-decoration: none;
}

.col a:hover,
.legal a:hover {
  color: var(--vp-c-brand-1);
}

.legal {
  display: flex;
  flex-wrap: wrap;
  justify-content: space-between;
  gap: 8px 24px;
  max-width: 1152px;
  margin: 0 auto;
  padding: 20px 24px 32px;
  border-top: 1px solid var(--vp-c-divider);
  color: var(--vp-c-text-3);
  font-size: 13px;
}

.legal p {
  margin: 0;
}

.legal a {
  font-size: 13px;
  text-decoration: underline;
}

@media (max-width: 768px) {
  .inner {
    grid-template-columns: repeat(3, 1fr);
    padding: 40px 16px 24px;
  }
  .brand {
    grid-column: 1 / -1;
  }
  .legal {
    padding: 16px 16px 28px;
  }
}

@media (max-width: 480px) {
  .inner {
    grid-template-columns: repeat(2, 1fr);
  }
}
</style>
