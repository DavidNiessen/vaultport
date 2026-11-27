<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script setup lang="ts">
/**
 * Downloads: the latest GitHub release fetched in the browser, grouped by platform, with the
 * visitor's OS and architecture pre-selected. Falls back to the releases page when GitHub is
 * unreachable or rate-limited, and to a "coming soon" state before the first release.
 */
import {
  ArrowUpRight,
  Download,
  FileCheck,
  Laptop,
  Monitor,
  Package,
  Smartphone,
  SquareTerminal,
  Tablet,
} from '@lucide/vue';
import { withBase } from 'vitepress';
import { computed, type Component } from 'vue';

import { RELEASES_URL } from '../../site';
import { OS_LABEL, usePlatform, type OsId } from '../composables/platform';
import {
  formatBytes,
  pickInstaller,
  useLatestRelease,
  type MatchedAsset,
} from '../composables/releases';

interface PlatformInfo {
  id: OsId;
  icon: Component;
  requirement: string;
  formats: string[];
  note: string;
}

const PLATFORMS: PlatformInfo[] = [
  {
    id: 'windows',
    icon: Monitor,
    requirement: 'Windows 10 or 11 · x64 or ARM64',
    formats: ['.exe installer', '.msi', 'x64', 'ARM64'],
    note: 'The installer sets ssh-client up for your user, no administrator rights needed. Use the MSI for managed deployments. WebView2 is installed automatically if it is missing.',
  },
  {
    id: 'macos',
    icon: Laptop,
    requirement: 'macOS 13 Ventura or later',
    formats: ['.dmg', 'Universal'],
    note: 'One universal disk image for Apple silicon and Intel Macs. Open it and drag ssh-client into Applications.',
  },
  {
    id: 'linux',
    icon: SquareTerminal,
    requirement: 'x86_64 or aarch64 · WebKitGTK 4.1',
    formats: ['AppImage', '.deb', '.rpm', 'x86_64', 'aarch64'],
    note: 'The .deb and .rpm packages pull in their dependencies. The AppImage runs on most distributions; it needs FUSE 2 (libfuse2 or libfuse2t64 on Ubuntu).',
  },
  {
    id: 'ios',
    icon: Tablet,
    requirement: 'iOS & iPadOS 16 or later',
    formats: ['.ipa'],
    note: 'ssh-client is not in the App Store. Install the IPA by sideloading (for example with AltStore or Sideloadly), or through TestFlight when a beta is offered.',
  },
  {
    id: 'android',
    icon: Smartphone,
    requirement: 'Android 8.0 (API 26) or later',
    formats: ['.apk', 'Universal'],
    note: 'Allow “Install unknown apps” for your browser or file manager, then open the APK.',
  },
];

const release = useLatestRelease();
const platform = usePlatform();

const ready = computed(() => (release.value.status === 'ready' ? release.value.release : null));

const byOs = computed(() => {
  const map = new Map<OsId, MatchedAsset[]>();
  for (const m of ready.value?.installers ?? []) {
    map.set(m.kind.os, [...(map.get(m.kind.os) ?? []), m]);
  }
  return map;
});

const recommended = computed(() => {
  const p = platform.value;
  if (!p?.os || !ready.value) return null;
  return pickInstaller(ready.value.installers, p.os, p.arch);
});

const ordered = computed(() => {
  const os = platform.value?.os;
  if (!os) return PLATFORMS;
  return [...PLATFORMS].sort((a, b) => Number(b.id === os) - Number(a.id === os));
});

const published = computed(() => {
  const at = ready.value?.publishedAt;
  if (!at) return '';
  return new Date(at).toLocaleDateString('en', { dateStyle: 'medium' });
});
</script>

<template>
  <div class="download">
    <header class="head">
      <div class="aura" aria-hidden="true" />
      <img class="logo" :src="withBase('/logo.svg')" alt="" width="72" height="72" />
      <h1 class="title">Download ssh-client</h1>
      <p class="lead">
        Free and open source for Windows, macOS, Linux, iOS &amp; iPadOS and Android. No account
        required — install it and create your vault.
      </p>

      <div class="status" role="status" aria-live="polite">
        <template v-if="release.status === 'loading'">
          <span class="skeleton" /> Looking up the latest release…
        </template>
        <template v-else-if="ready">
          <span class="dot ok" />
          Version <strong>{{ ready.version }}</strong>
          <template v-if="ready.prerelease"> (pre-release)</template>
          <template v-if="published"> · {{ published }}</template>
          ·
          <a :href="ready.url" target="_blank" rel="noopener">Release notes</a>
        </template>
        <template v-else-if="release.status === 'none'">
          <span class="dot warn" /> The first release is on its way.
        </template>
        <template v-else>
          <span class="dot warn" />
          {{
            release.status === 'unavailable' && release.reason === 'rate-limited'
              ? 'GitHub is rate-limiting requests from your network right now.'
              : 'GitHub could not be reached.'
          }}
          <a :href="RELEASES_URL" target="_blank" rel="noopener">Browse all releases</a>
        </template>
      </div>
    </header>

    <section v-if="recommended && platform?.os" class="recommended" aria-labelledby="rec-title">
      <div>
        <p class="rec-kicker" id="rec-title">Recommended for {{ OS_LABEL[platform.os] }}</p>
        <p class="rec-file">{{ recommended.asset.name }}</p>
      </div>
      <a class="btn primary" :href="recommended.asset.browser_download_url">
        <Download :size="18" aria-hidden="true" />
        Download {{ recommended.kind.label }}
        <span class="size">{{ formatBytes(recommended.asset.size) }}</span>
      </a>
    </section>

    <section v-if="release.status === 'none'" class="soon">
      <Package :size="22" aria-hidden="true" />
      <div>
        <h2>Coming soon</h2>
        <p>
          There is no published release yet. Watch the
          <a :href="RELEASES_URL" target="_blank" rel="noopener">releases page</a> on GitHub, or
          <a :href="withBase('/development/building')">build ssh-client from source</a> today.
        </p>
      </div>
    </section>

    <ul class="platforms">
      <li
        v-for="p in ordered"
        :id="p.id"
        :key="p.id"
        class="platform"
        :class="{ current: platform?.os === p.id }"
      >
        <div class="p-head">
          <span class="p-icon"><component :is="p.icon" :size="20" aria-hidden="true" /></span>
          <div>
            <h2 class="p-name">
              {{ OS_LABEL[p.id] }}
              <span v-if="platform?.os === p.id" class="you">Your device</span>
            </h2>
            <p class="p-req">{{ p.requirement }}</p>
          </div>
        </div>

        <ul v-if="byOs.get(p.id)?.length" class="assets">
          <li v-for="m in byOs.get(p.id)" :key="m.asset.name">
            <a class="asset" :href="m.asset.browser_download_url">
              <Download :size="16" class="a-icon" aria-hidden="true" />
              <span class="a-text">
                <span class="a-label">{{ m.kind.label }}</span>
                <span class="a-detail">{{ m.kind.detail }}</span>
              </span>
              <span class="a-size">{{ formatBytes(m.asset.size) }}</span>
            </a>
          </li>
        </ul>
        <div v-else class="empty">
          <template v-if="ready">Not part of this release.</template>
          <template v-else-if="release.status === 'loading'">Checking…</template>
          <span v-else class="formats">
            <span class="soon-label">Coming soon</span>
            <span v-for="f in p.formats" :key="f" class="format">{{ f }}</span>
          </span>
          <a :href="RELEASES_URL" target="_blank" rel="noopener" class="empty-link">
            All releases <ArrowUpRight :size="13" aria-hidden="true" />
          </a>
        </div>

        <p class="p-note">{{ p.note }}</p>
      </li>
    </ul>

    <section v-if="ready" class="integrity" aria-label="Release integrity files">
      <FileCheck :size="20" aria-hidden="true" />
      <p>
        <a v-if="ready.checksums" :href="ready.checksums.browser_download_url">{{
          ready.checksums.name
        }}</a>
        <template v-if="ready.checksums && ready.sbom"> · </template>
        <a v-if="ready.sbom" :href="ready.sbom.browser_download_url">{{ ready.sbom.name }}</a>
        <template v-if="ready.signatures.length">
          · {{ ready.signatures.length }} signature files
        </template>
        · <a :href="ready.url" target="_blank" rel="noopener">all {{ ready.assetCount }} files</a>
      </p>
    </section>
  </div>
</template>

<style scoped>
.download {
  max-width: 1152px;
  margin: 0 auto;
  padding: 56px 24px 24px;
  overflow-x: clip;
}

.head {
  position: relative;
  isolation: isolate;
  text-align: center;
}

.aura {
  position: absolute;
  top: -120px;
  left: 50%;
  z-index: -1;
  width: 720px;
  height: 420px;
  margin-left: -360px;
  background: radial-gradient(closest-side, var(--sc-glow), transparent);
  filter: blur(40px);
  pointer-events: none;
}

.logo {
  display: block;
  margin: 0 auto 20px;
  filter: drop-shadow(0 16px 32px rgb(99 102 241 / 0.45));
}

.title {
  margin: 0;
  color: var(--vp-c-text-1);
  font-size: clamp(34px, 5vw, 52px);
  font-weight: 700;
  letter-spacing: -0.03em;
  line-height: 1.1;
}

.lead {
  max-width: 620px;
  margin: 16px auto 0;
  color: var(--vp-c-text-2);
  font-size: 17px;
  line-height: 1.6;
}

.status {
  display: inline-flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: center;
  gap: 6px;
  margin-top: 24px;
  padding: 8px 16px;
  border-radius: 999px;
  background: var(--vp-c-bg-soft);
  box-shadow: inset 0 0 0 1px var(--sc-border);
  color: var(--vp-c-text-2);
  font-size: 14px;
}

.status a {
  color: var(--vp-c-brand-1);
  font-weight: 500;
}

.status strong {
  color: var(--vp-c-text-1);
}

.dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
}

.dot.ok {
  background: var(--sc-success);
}

.dot.warn {
  background: #f59e0b;
}

.skeleton {
  width: 14px;
  height: 14px;
  border: 2px solid var(--sc-border-strong);
  border-top-color: var(--sc-accent);
  border-radius: 50%;
  animation: spin 0.9s linear infinite;
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}

.recommended {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  margin-top: 40px;
  padding: 24px 28px;
  border-radius: 20px;
  background:
    radial-gradient(120% 140% at 0% 0%, rgb(99 102 241 / 0.18), transparent 60%), var(--sc-surface);
  box-shadow: 0 0 0 1px rgb(99 102 241 / 0.4);
}

.rec-kicker {
  margin: 0;
  color: var(--vp-c-text-1);
  font-size: 18px;
  font-weight: 600;
}

.rec-file {
  margin: 4px 0 0;
  color: var(--vp-c-text-3);
  font-family: var(--vp-font-family-mono);
  font-size: 13px;
  word-break: break-all;
}

.btn {
  display: inline-flex;
  align-items: center;
  gap: 10px;
  height: 48px;
  padding: 0 22px;
  border-radius: 12px;
  font-size: 15px;
  font-weight: 600;
  text-decoration: none;
}

.btn.primary {
  background: #6366f1;
  color: #fff;
  box-shadow: 0 12px 32px -12px rgb(99 102 241 / 0.8);
}

.btn.primary:hover {
  background: #5558ee;
}

.size {
  padding-left: 10px;
  border-left: 1px solid rgb(255 255 255 / 0.3);
  font-weight: 500;
  opacity: 0.85;
}

.soon {
  display: flex;
  gap: 16px;
  margin-top: 40px;
  padding: 24px 28px;
  border-radius: 20px;
  background: var(--sc-surface);
  box-shadow: 0 0 0 1px var(--sc-border);
  color: var(--sc-accent);
}

.soon > svg {
  flex-shrink: 0;
  margin-top: 2px;
}

.soon h2 {
  margin: 0;
  color: var(--vp-c-text-1);
  font-size: 18px;
}

.soon p {
  margin: 6px 0 0;
  color: var(--vp-c-text-2);
  font-size: 15px;
  line-height: 1.6;
}

.soon a {
  color: var(--vp-c-brand-1);
}

.platforms {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 16px;
  margin: 32px 0 0;
  padding: 0;
  list-style: none;
}

.platform {
  display: flex;
  flex-direction: column;
  gap: 16px;
  padding: 24px;
  border-radius: 20px;
  background: var(--sc-surface);
  box-shadow: 0 0 0 1px var(--sc-border);
  scroll-margin-top: calc(var(--vp-nav-height) + 24px);
}

.platform.current,
.platform:target {
  box-shadow:
    0 0 0 1px rgb(99 102 241 / 0.55),
    0 24px 48px -28px rgb(99 102 241 / 0.6);
}

.p-head {
  display: flex;
  align-items: center;
  gap: 14px;
}

.p-icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 42px;
  height: 42px;
  border-radius: 12px;
  background: var(--vp-c-brand-soft);
  color: var(--sc-accent);
}

.p-name {
  display: flex;
  align-items: center;
  gap: 8px;
  margin: 0;
  color: var(--vp-c-text-1);
  font-size: 18px;
  font-weight: 600;
}

.you {
  padding: 2px 8px;
  border-radius: 999px;
  background: var(--vp-c-brand-soft);
  color: var(--vp-c-brand-1);
  font-size: 11.5px;
  font-weight: 600;
}

.p-req {
  margin: 2px 0 0;
  color: var(--vp-c-text-3);
  font-size: 13.5px;
}

.assets {
  display: grid;
  gap: 8px;
  margin: 0;
  padding: 0;
  list-style: none;
}

.asset {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 12px 14px;
  border-radius: 12px;
  background: var(--vp-c-bg-soft);
  box-shadow: inset 0 0 0 1px var(--sc-border);
  color: inherit;
  text-decoration: none;
  transition: box-shadow 0.2s;
}

.asset:hover {
  box-shadow: inset 0 0 0 1px var(--vp-c-brand-2);
}

.a-icon {
  flex-shrink: 0;
  color: var(--sc-accent);
}

.a-text {
  display: grid;
  flex: 1;
  min-width: 0;
}

.a-label {
  color: var(--vp-c-text-1);
  font-size: 14.5px;
  font-weight: 600;
}

.a-detail {
  color: var(--vp-c-text-3);
  font-size: 12.5px;
}

.a-size {
  color: var(--vp-c-text-2);
  font-family: var(--vp-font-family-mono);
  font-size: 12.5px;
  white-space: nowrap;
}

.formats {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 6px;
}

.soon-label {
  margin-right: 4px;
  color: var(--vp-c-text-1);
  font-weight: 500;
}

.format {
  padding: 2px 8px;
  border-radius: 999px;
  box-shadow: inset 0 0 0 1px var(--sc-border-strong);
  color: var(--vp-c-text-2);
  font-family: var(--vp-font-family-mono);
  font-size: 12px;
}

.empty {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  padding: 12px 14px;
  border-radius: 12px;
  background: var(--vp-c-bg-soft);
  color: var(--vp-c-text-2);
  font-size: 13.5px;
}

.empty-link {
  display: inline-flex;
  align-items: center;
  gap: 2px;
  color: var(--vp-c-brand-1);
  font-weight: 500;
  text-decoration: none;
}

.p-note {
  margin: auto 0 0;
  color: var(--vp-c-text-2);
  font-size: 13.5px;
  line-height: 1.6;
}

.integrity {
  display: flex;
  align-items: center;
  gap: 12px;
  margin-top: 24px;
  color: var(--sc-accent);
  font-size: 14px;
}

.integrity p {
  margin: 0;
  color: var(--vp-c-text-2);
}

.integrity a {
  color: var(--vp-c-brand-1);
  font-family: var(--vp-font-family-mono);
  font-size: 13px;
}

@media (max-width: 768px) {
  .platforms {
    grid-template-columns: 1fr;
  }
}

@media (max-width: 640px) {
  .download {
    padding: 40px 16px 16px;
  }
  .recommended,
  .soon {
    padding: 20px;
  }
  .recommended .btn {
    width: 100%;
    justify-content: center;
  }
  .platform {
    padding: 20px;
  }
}

@media (prefers-reduced-motion: reduce) {
  .skeleton {
    animation: none;
  }
}
</style>
