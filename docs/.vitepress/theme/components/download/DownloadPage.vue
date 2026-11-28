<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script setup lang="ts">
/**
 * Downloads: the latest GitHub release, fetched in the browser and grouped by platform. The
 * visitor's OS is pre-selected in a sliding platform switcher, with the recommended file up
 * front and the app itself shown next to it. Falls back to the releases page when GitHub is
 * unreachable or rate-limited, and to "coming soon" before the first release.
 */
import {
  ArrowUpRight,
  Check,
  Download,
  FileCheck,
  Hammer,
  Laptop,
  Monitor,
  Smartphone,
  SquareTerminal,
  Tablet,
} from '@lucide/vue';
import { withBase } from 'vitepress';
import { computed, ref, watch, type Component } from 'vue';

import { RELEASES_URL } from '../../../site';
import { OS_LABEL, usePlatform, type OsId } from '../../composables/platform';
import {
  formatBytes,
  pickInstaller,
  useLatestRelease,
  type MatchedAsset,
} from '../../composables/releases';

interface PlatformInfo {
  id: OsId;
  short: string;
  icon: Component;
  requirement: string;
  note: string;
  mobile: boolean;
}

const PLATFORMS: PlatformInfo[] = [
  {
    id: 'windows',
    short: 'Windows',
    icon: Monitor,
    requirement: 'Windows 10 or 11 · x64 or ARM64',
    note: 'The installer sets Vaultport up for your user, no administrator rights needed. Use the MSI for managed deployments. WebView2 is installed automatically if it is missing.',
    mobile: false,
  },
  {
    id: 'macos',
    short: 'macOS',
    icon: Laptop,
    requirement: 'macOS 13 Ventura or later',
    note: 'One universal disk image for Apple silicon and Intel Macs. Open it and drag Vaultport into Applications.',
    mobile: false,
  },
  {
    id: 'linux',
    short: 'Linux',
    icon: SquareTerminal,
    requirement: 'x86_64 or aarch64 · WebKitGTK 4.1',
    note: 'The .deb and .rpm packages pull in their dependencies. The AppImage runs on most distributions; it needs FUSE 2 (libfuse2 or libfuse2t64 on Ubuntu).',
    mobile: false,
  },
  {
    id: 'android',
    short: 'Android',
    icon: Smartphone,
    requirement: 'Android 8.0 (API 26) or later',
    note: 'Allow “Install unknown apps” for your browser or file manager, then open the APK.',
    mobile: true,
  },
  {
    id: 'ios',
    short: 'iOS',
    icon: Tablet,
    requirement: 'iOS & iPadOS 16 or later',
    note: 'Vaultport is not in the App Store. When a release is signed, the IPA reaches devices through TestFlight (when a beta is offered) or by re-signing it with a sideloading tool such as AltStore or Sideloadly.',
    mobile: true,
  },
];

const release = useLatestRelease();
const platform = usePlatform();
const selected = ref<OsId>('macos');
const touched = ref(false);

watch(platform, (p) => {
  if (p?.os && !touched.value) selected.value = p.os;
});
function choose(id: OsId) {
  touched.value = true;
  selected.value = id;
}

const index = computed(() => PLATFORMS.findIndex((p) => p.id === selected.value));
const info = computed(() => PLATFORMS[index.value]!);
const ready = computed(() => (release.value.status === 'ready' ? release.value.release : null));

const files = computed<MatchedAsset[]>(
  () => ready.value?.installers.filter((m) => m.kind.os === selected.value) ?? [],
);
const recommended = computed(() => {
  if (!ready.value) return null;
  const arch = platform.value?.os === selected.value ? platform.value.arch : 'x64';
  return pickInstaller(ready.value.installers, selected.value, arch);
});
const published = computed(() => {
  const at = ready.value?.publishedAt;
  return at ? new Date(at).toLocaleDateString('en', { dateStyle: 'medium' }) : '';
});
</script>

<template>
  <div class="download">
    <section class="hero">
      <div class="aurora" aria-hidden="true" />
      <div class="hero-grid container">
        <div class="copy">
          <p class="eyebrow" v-reveal>Download</p>
          <h1 class="display" v-reveal="60">
            Get Vaultport <span class="serif grad-text">for {{ info.short }}.</span>
          </h1>
          <p class="lede" v-reveal="120">
            Free and open source. No account — install it and create your vault.
          </p>

          <div class="status mono" role="status" aria-live="polite" v-reveal="160">
            <template v-if="release.status === 'loading'">
              <span class="pulse" /> Looking up the latest release…
            </template>
            <template v-else-if="ready">
              <span class="dot ok" /> v{{ ready.version
              }}<template v-if="ready.prerelease"> · pre-release</template
              ><template v-if="published"> · {{ published }}</template> ·
              <a :href="ready.url" target="_blank" rel="noreferrer">release notes</a>
            </template>
            <template v-else-if="release.status === 'none'">
              <span class="dot warn" /> The first release is on its way
            </template>
            <template v-else>
              <span class="dot warn" />
              {{
                release.status === 'unavailable' && release.reason === 'rate-limited'
                  ? 'GitHub is rate-limiting your network'
                  : 'GitHub could not be reached'
              }}
              · <a :href="RELEASES_URL" target="_blank" rel="noreferrer">all releases</a>
            </template>
          </div>

          <div class="switch" role="group" aria-label="Platform" v-reveal="200">
            <span
              class="switch-pill"
              :style="{ transform: `translateX(${index * 100}%)` }"
              aria-hidden="true"
            />
            <button
              v-for="p in PLATFORMS"
              :key="p.id"
              type="button"
              class="switch-btn"
              :class="{ on: p.id === selected }"
              :aria-pressed="p.id === selected"
              @click="choose(p.id)"
            >
              <component :is="p.icon" :size="16" aria-hidden="true" />
              <span>{{ p.short }}</span>
              <span v-if="platform?.os === p.id" class="you" title="Your device" />
            </button>
          </div>

          <Transition name="swap" mode="out-in">
            <div :key="selected" class="panel">
              <p class="req"><Check :size="15" aria-hidden="true" /> {{ info.requirement }}</p>

              <a
                v-if="recommended"
                v-magnetic="0.15"
                :href="recommended.asset.browser_download_url"
                class="btn btn-primary btn-lg big-dl"
              >
                <Download aria-hidden="true" />
                Download {{ recommended.kind.label }}
                <span class="size mono">{{ formatBytes(recommended.asset.size) }}</span>
              </a>
              <div v-else-if="release.status === 'loading'" class="skeleton" />
              <div v-else-if="release.status === 'none'" class="soon">
                <Hammer :size="18" aria-hidden="true" />
                <p>
                  No published release yet. Watch the
                  <a :href="RELEASES_URL" target="_blank" rel="noreferrer">releases page</a> or
                  <a :href="withBase('/development/building')">build it from source</a> today.
                </p>
              </div>
              <a v-else :href="RELEASES_URL" target="_blank" rel="noreferrer" class="btn btn-lg">
                Open the releases page <ArrowUpRight aria-hidden="true" />
              </a>

              <ul v-if="files.length > 1" class="files">
                <li v-for="m in files" :key="m.asset.name">
                  <a :href="m.asset.browser_download_url" class="file">
                    <span class="file-main">
                      <strong>{{ m.kind.label }}</strong>
                      <span class="mono">{{ m.kind.detail }}</span>
                    </span>
                    <span class="file-size mono">{{ formatBytes(m.asset.size) }}</span>
                    <Download :size="16" class="file-icon" aria-hidden="true" />
                  </a>
                </li>
              </ul>
              <p class="note">{{ info.note }}</p>
            </div>
          </Transition>
        </div>

        <div class="showcase" v-reveal="160">
          <Transition name="device" mode="out-in">
            <div v-if="info.mobile" key="mobile" class="phones">
              <img
                :src="withBase('/screens/phone-hosts.webp')"
                alt="Vaultport's hosts list on a phone"
                width="780"
                height="1688"
              />
              <img
                :src="withBase('/screens/phone-terminal.webp')"
                alt="A terminal session on a phone"
                width="780"
                height="1688"
              />
            </div>
            <div v-else key="desktop" v-tilt="5" class="desk">
              <img
                :src="withBase('/screens/hosts.webp')"
                alt="Vaultport's hosts view on the desktop"
                width="2000"
                height="1250"
              />
              <span class="glare" aria-hidden="true" />
            </div>
          </Transition>
        </div>
      </div>
    </section>

    <section class="extras container">
      <a v-if="ready?.checksums" :href="ready.checksums.browser_download_url" class="extra">
        <FileCheck :size="18" aria-hidden="true" />
        <span><strong>SHA256SUMS.txt</strong><em>Checksums of every file</em></span>
      </a>
      <a v-if="ready?.sbom" :href="ready.sbom.browser_download_url" class="extra">
        <FileCheck :size="18" aria-hidden="true" />
        <span><strong>SBOM</strong><em>SPDX bill of materials</em></span>
      </a>
      <a :href="RELEASES_URL" target="_blank" rel="noreferrer" class="extra">
        <ArrowUpRight :size="18" aria-hidden="true" />
        <span><strong>All releases</strong><em>Older versions and notes</em></span>
      </a>
      <a :href="withBase('/development/building')" class="extra">
        <Hammer :size="18" aria-hidden="true" />
        <span><strong>Build from source</strong><em>Rust, Node.js and pnpm</em></span>
      </a>
    </section>
  </div>
</template>

<style scoped>
.download {
  position: relative;
}
.hero {
  position: relative;
  padding: calc(var(--nav-h) + 72px) 0 40px;
  isolation: isolate;
}
.aurora {
  position: absolute;
  inset: 0 0 -20%;
  z-index: -1;
  background:
    radial-gradient(40% 50% at 20% 10%, rgb(var(--glow-1) / 0.3), transparent 70%),
    radial-gradient(35% 45% at 85% 25%, rgb(var(--glow-2) / 0.22), transparent 70%),
    radial-gradient(30% 30% at 60% 80%, rgb(var(--glow-3) / 0.1), transparent 70%);
  filter: blur(10px);
  animation: drift 18s var(--ease-in-out) infinite alternate;
}
@keyframes drift {
  to {
    transform: translate3d(2%, 3%, 0) scale(1.05);
  }
}
.hero-grid {
  display: grid;
  grid-template-columns: minmax(0, 1fr) minmax(0, 1.05fr);
  gap: clamp(32px, 5vw, 72px);
  align-items: center;
}
.copy {
  display: grid;
  gap: 20px;
  justify-items: start;
}
.display {
  font-size: clamp(42px, 5.6vw, 76px);
}
.status {
  display: inline-flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px;
  padding: 6px 14px;
  border-radius: 999px;
  border: 1px solid var(--border);
  background: var(--surface);
  font-size: 12.5px;
  color: var(--text-2);
}
.status a {
  color: var(--accent);
}
.dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
}
.dot.ok {
  background: var(--ok);
  box-shadow: 0 0 10px var(--ok);
}
.dot.warn {
  background: var(--warn);
}
.pulse {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: var(--accent);
  animation: pulse 1s infinite alternate;
}
@keyframes pulse {
  to {
    opacity: 0.2;
  }
}

.switch {
  position: relative;
  display: grid;
  grid-template-columns: repeat(5, 1fr);
  width: min(100%, 560px);
  padding: 4px;
  border-radius: 16px;
  border: 1px solid var(--border);
  background: var(--bg-soft);
}
.switch-pill {
  position: absolute;
  top: 4px;
  bottom: 4px;
  left: 4px;
  width: calc((100% - 8px) / 5);
  border-radius: 12px;
  background: var(--text);
  box-shadow: 0 8px 24px -8px rgb(var(--glow-1) / 0.6);
  transition: transform 0.55s var(--ease-spring);
}
.switch-btn {
  position: relative;
  display: grid;
  justify-items: center;
  gap: 4px;
  padding: 10px 4px;
  border-radius: 12px;
  font-size: 13px;
  font-weight: 550;
  color: var(--text-2);
  transition: color 0.3s;
}
.switch-btn:hover {
  color: var(--text);
}
.switch-btn.on {
  color: var(--bg);
}
.you {
  position: absolute;
  top: 7px;
  right: 10px;
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--ok);
  box-shadow: 0 0 8px var(--ok);
}
.panel {
  display: grid;
  gap: 16px;
  width: min(100%, 560px);
}
.req {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 14px;
  color: var(--text-2);
}
.req svg {
  color: var(--ok);
}
.big-dl {
  justify-self: start;
}
.size {
  padding: 2px 8px;
  border-radius: 999px;
  background: rgb(0 0 0 / 0.08);
  font-size: 12px;
}
.skeleton {
  width: 280px;
  height: 56px;
  border-radius: 999px;
  background: linear-gradient(90deg, var(--surface), var(--surface-3), var(--surface));
  background-size: 200% 100%;
  animation: shimmer 1.2s linear infinite;
}
@keyframes shimmer {
  to {
    background-position: -200% 0;
  }
}
.soon {
  display: flex;
  gap: 12px;
  padding: 16px 18px;
  border-radius: var(--radius);
  border: 1px dashed var(--border-strong);
  font-size: 14.5px;
  color: var(--text-2);
}
.soon svg {
  flex: none;
  color: var(--accent);
  margin-top: 2px;
}
.soon a {
  color: var(--accent);
  text-decoration: underline;
  text-underline-offset: 3px;
}
.files {
  list-style: none;
  margin: 0;
  padding: 0;
  display: grid;
  gap: 8px;
}
.file {
  display: flex;
  align-items: center;
  gap: 14px;
  padding: 12px 16px;
  border-radius: 14px;
  border: 1px solid var(--border);
  background: var(--surface);
  transition:
    border-color 0.25s,
    background-color 0.25s,
    transform 0.4s var(--ease-out);
}
.file:hover {
  border-color: color-mix(in oklab, var(--accent) 50%, transparent);
  background: var(--accent-soft);
  transform: translateX(4px);
}
.file-main {
  display: grid;
  flex: 1;
  font-size: 14px;
}
.file-main .mono {
  font-size: 12px;
  color: var(--text-3);
}
.file-size {
  font-size: 12px;
  color: var(--text-3);
}
.file-icon {
  color: var(--accent);
}
.note {
  font-size: 14px;
  line-height: 1.6;
  color: var(--text-3);
}
.swap-enter-active,
.swap-leave-active {
  transition:
    opacity 0.25s,
    transform 0.35s var(--ease-out),
    filter 0.3s;
}
.swap-enter-from {
  opacity: 0;
  transform: translateY(10px);
  filter: blur(4px);
}
.swap-leave-to {
  opacity: 0;
  transform: translateY(-6px);
  filter: blur(4px);
}

.showcase {
  position: sticky;
  top: calc(var(--nav-h) + 96px);
  align-self: start;
  margin-top: 40px;
  perspective: 1400px;
}
.desk {
  position: relative;
  border-radius: 16px;
  padding: 5px;
  background: linear-gradient(180deg, rgb(255 255 255 / 0.16), rgb(255 255 255 / 0.03));
  box-shadow:
    var(--shadow-lg),
    0 0 120px -30px rgb(var(--glow-1) / 0.6);
  transform-style: preserve-3d;
}
.desk img {
  width: 100%;
  height: auto;
  border-radius: 12px;
}
.glare {
  position: absolute;
  inset: 0;
  border-radius: inherit;
  background: radial-gradient(
    circle at var(--glare-x, 50%) var(--glare-y, 0%),
    rgb(255 255 255 / 0.12),
    transparent 50%
  );
  pointer-events: none;
}
.phones {
  display: flex;
  justify-content: center;
  gap: 5%;
}
.phones img {
  width: 40%;
  height: auto;
  border-radius: 26px;
  border: 1px solid var(--border-strong);
  box-shadow:
    var(--shadow-lg),
    0 0 80px -20px rgb(var(--glow-1) / 0.6);
}
.phones img:first-child {
  transform: rotate(-5deg) translateY(10px);
}
.phones img:last-child {
  transform: rotate(5deg) translateY(-10px);
}
.device-enter-active,
.device-leave-active {
  transition:
    opacity 0.35s,
    transform 0.5s var(--ease-out),
    filter 0.35s;
}
.device-enter-from {
  opacity: 0;
  transform: translateY(30px) rotateX(12deg) scale(0.95);
  filter: blur(8px);
}
.device-leave-to {
  opacity: 0;
  transform: translateY(-20px) scale(0.97);
  filter: blur(8px);
}

.extras {
  display: grid;
  grid-template-columns: repeat(4, 1fr);
  gap: 12px;
  margin-top: 40px;
}
.extra {
  display: flex;
  align-items: center;
  gap: 14px;
  padding: 16px 18px;
  border-radius: var(--radius);
  border: 1px solid var(--border);
  background: var(--surface);
  transition:
    border-color 0.25s,
    transform 0.4s var(--ease-out);
}
.extra:hover {
  border-color: var(--border-strong);
  transform: translateY(-2px);
}
.extra svg {
  flex: none;
  color: var(--accent);
}
.extra span {
  display: grid;
}
.extra strong {
  font-size: 14.5px;
}
.extra em {
  font-style: normal;
  font-size: 12.5px;
  color: var(--text-3);
}

@media (max-width: 960px) {
  .hero-grid {
    grid-template-columns: 1fr;
  }
  .showcase {
    position: relative;
    top: 0;
  }
  .extras {
    grid-template-columns: 1fr 1fr;
  }
}
@media (max-width: 520px) {
  .switch-btn span:not(.you) {
    font-size: 12px;
  }
  .extras {
    grid-template-columns: 1fr;
  }
}
</style>
