<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script setup lang="ts">
/**
 * Serverless sync, animated: sealed packets travel from each device to storage the visitor
 * picks (folder, WebDAV or S3) and back out to the others. The project's own cloud is drawn
 * crossed out, because it does not exist.
 */
import { Cloud, Folder, Globe, HardDrive, Laptop, Monitor, Smartphone } from '@lucide/vue';
import { computed, ref, type Component } from 'vue';

interface Backend {
  id: string;
  label: string;
  icon: Component;
  examples: string;
}
const BACKENDS: Backend[] = [
  {
    id: 'folder',
    label: 'Synced folder',
    icon: Folder,
    examples: 'iCloud Drive · Dropbox · Syncthing · OneDrive',
  },
  {
    id: 'webdav',
    label: 'WebDAV',
    icon: Globe,
    examples: 'Nextcloud · ownCloud · any WebDAV server',
  },
  {
    id: 's3',
    label: 'S3-compatible',
    icon: HardDrive,
    examples: 'AWS S3 · Cloudflare R2 · MinIO · Backblaze B2',
  },
];
const backend = ref(0);
const current = computed(() => BACKENDS[backend.value]!);
const hovered = ref<number | null>(null);

const DEVICES = [
  { icon: Laptop, label: 'Laptop', x: 90, y: 90, path: 'M130 110 C 230 110, 260 210, 360 210' },
  { icon: Smartphone, label: 'Phone', x: 90, y: 330, path: 'M130 330 C 230 330, 260 230, 360 230' },
  { icon: Monitor, label: 'Desktop', x: 710, y: 210, path: 'M670 220 C 600 220, 560 220, 500 220' },
];
</script>

<template>
  <section class="sync">
    <div class="container">
      <div class="head">
        <p class="eyebrow" v-reveal>Serverless sync</p>
        <h2 class="h-section" v-reveal="80">
          Your devices talk through <span class="serif grad-text">your storage.</span>
        </h2>
        <p class="lede" v-reveal="160">
          Changes are sealed on the device before they leave it. Your storage only ever holds
          ciphertext, and hash chains catch rollbacks and missing pieces. There is no Vaultport
          cloud to breach — or to shut down.
        </p>
      </div>

      <div class="panel" v-reveal="120">
        <div class="tabs" role="tablist" aria-label="Storage backend">
          <span
            class="tab-pill"
            :style="{ transform: `translateX(${backend * 100}%)` }"
            aria-hidden="true"
          />
          <button
            v-for="(b, i) in BACKENDS"
            :key="b.id"
            type="button"
            role="tab"
            :aria-selected="i === backend"
            class="tab"
            :class="{ on: i === backend }"
            @click="backend = i"
          >
            <component :is="b.icon" :size="15" /> {{ b.label }}
          </button>
        </div>

        <div class="diagram">
          <svg viewBox="0 0 800 440" class="wires" aria-hidden="true">
            <path
              v-for="(d, i) in DEVICES"
              :id="`sync-wire-${i}`"
              :key="d.label"
              :d="d.path"
              class="wire"
              :class="{ hot: hovered === i }"
            />
            <template v-for="(d, i) in DEVICES" :key="`p-${d.label}`">
              <g class="packet">
                <rect x="-11" y="-8" width="22" height="16" rx="5" />
                <path d="M-3 -1 v-2 a3 3 0 0 1 6 0 v2" class="shackle" />
                <animateMotion
                  :dur="`${3 + i * 0.4}s`"
                  :begin="`${i * 0.7}s`"
                  repeatCount="indefinite"
                  rotate="0"
                >
                  <mpath :href="`#sync-wire-${i}`" />
                </animateMotion>
              </g>
              <g class="packet back">
                <rect x="-11" y="-8" width="22" height="16" rx="5" />
                <animateMotion
                  :dur="`${3.4 + i * 0.3}s`"
                  :begin="`${1.5 + i * 0.5}s`"
                  repeatCount="indefinite"
                  keyPoints="1;0"
                  keyTimes="0;1"
                  calcMode="linear"
                >
                  <mpath :href="`#sync-wire-${i}`" />
                </animateMotion>
              </g>
            </template>
          </svg>

          <div
            v-for="(d, i) in DEVICES"
            :key="d.label"
            class="device"
            :style="{ left: `${(d.x / 800) * 100}%`, top: `${(d.y / 440) * 100}%` }"
            @pointerenter="hovered = i"
            @pointerleave="hovered = null"
          >
            <span class="device-icon"><component :is="d.icon" :size="22" /></span>
            <span class="device-label">{{ d.label }}</span>
            <span class="device-sub mono">encrypts locally</span>
          </div>

          <div class="storage">
            <Transition name="swap" mode="out-in">
              <div :key="current.id" class="storage-inner">
                <span class="storage-icon"><component :is="current.icon" :size="28" /></span>
                <strong>{{ current.label }}</strong>
                <span class="mono storage-ex">{{ current.examples }}</span>
              </div>
            </Transition>
            <span class="storage-note mono">sees ciphertext only</span>
          </div>

          <div class="nocloud" aria-label="No Vaultport servers">
            <Cloud :size="20" />
            <span>Vaultport servers</span>
            <b class="mono">none</b>
          </div>
        </div>
      </div>
    </div>
  </section>
</template>

<style scoped>
.sync {
  padding: 120px 0 60px;
}
.head {
  display: grid;
  gap: 18px;
  max-width: 760px;
  margin-bottom: 40px;
}
.panel {
  position: relative;
  border-radius: var(--radius-xl);
  border: 1px solid var(--border);
  background:
    radial-gradient(50% 60% at 55% 50%, rgb(var(--glow-1) / 0.14), transparent 70%),
    linear-gradient(180deg, var(--surface-2), var(--surface));
  padding: 20px;
  overflow: hidden;
}
.panel::before {
  content: '';
  position: absolute;
  inset: 0;
  background-image: radial-gradient(var(--border) 1px, transparent 1px);
  background-size: 22px 22px;
  mask-image: radial-gradient(60% 60% at 50% 50%, #000, transparent);
  -webkit-mask-image: radial-gradient(60% 60% at 50% 50%, #000, transparent);
  pointer-events: none;
}
.tabs {
  position: relative;
  display: inline-grid;
  grid-template-columns: repeat(3, 1fr);
  padding: 4px;
  border-radius: 999px;
  border: 1px solid var(--border);
  background: var(--bg-soft);
  z-index: 1;
}
.tab-pill {
  position: absolute;
  top: 4px;
  bottom: 4px;
  left: 4px;
  width: calc((100% - 8px) / 3);
  border-radius: 999px;
  background: var(--text);
  transition: transform 0.5s var(--ease-out);
}
.tab {
  position: relative;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  padding: 8px 16px;
  border-radius: 999px;
  font-size: 13.5px;
  font-weight: 550;
  color: var(--text-2);
  white-space: nowrap;
  transition: color 0.3s;
}
.tab.on {
  color: var(--bg);
}
.diagram {
  position: relative;
  aspect-ratio: 800 / 440;
  margin-top: 8px;
}
.wires {
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
  overflow: visible;
}
.wire {
  fill: none;
  stroke: var(--border-strong);
  stroke-width: 2;
  stroke-dasharray: 3 6;
  transition: stroke 0.3s;
}
.wire.hot {
  stroke: var(--accent);
}
.packet rect {
  fill: var(--accent);
  filter: drop-shadow(0 0 8px rgb(var(--glow-1) / 0.9));
}
.packet .shackle {
  fill: none;
  stroke: var(--on-accent);
  stroke-width: 1.6;
}
.packet.back rect {
  fill: var(--accent-2);
  filter: drop-shadow(0 0 8px rgb(var(--glow-2) / 0.9));
}
.device {
  position: absolute;
  display: grid;
  justify-items: center;
  gap: 6px;
  transform: translate(-50%, -50%);
  text-align: center;
  cursor: default;
}
.device-icon {
  display: grid;
  place-items: center;
  width: 64px;
  height: 64px;
  border-radius: 20px;
  border: 1px solid var(--border-strong);
  background: var(--bg-elev);
  box-shadow: var(--shadow-md);
  transition:
    transform 0.4s var(--ease-spring),
    border-color 0.3s,
    box-shadow 0.3s;
}
.device:hover .device-icon {
  transform: translateY(-4px) scale(1.06);
  border-color: var(--accent);
  box-shadow: 0 0 30px rgb(var(--glow-1) / 0.5);
}
.device-label {
  font-size: 14px;
  font-weight: 600;
}
.device-sub {
  font-size: 10.5px;
  color: var(--text-3);
}
.storage {
  position: absolute;
  left: 53.75%;
  top: 50%;
  transform: translate(-50%, -50%);
  display: grid;
  justify-items: center;
  gap: 10px;
  width: min(34%, 260px);
}
.storage-inner {
  display: grid;
  justify-items: center;
  gap: 6px;
  width: 100%;
  padding: 22px 16px;
  border-radius: 24px;
  border: 1px solid color-mix(in oklab, var(--accent) 45%, transparent);
  background: color-mix(in oklab, var(--bg-elev) 85%, transparent);
  box-shadow:
    0 0 60px rgb(var(--glow-1) / 0.35),
    var(--shadow-md);
  text-align: center;
  backdrop-filter: blur(8px);
}
.storage-icon {
  color: var(--accent);
}
.storage-inner strong {
  font-size: 16px;
  letter-spacing: -0.02em;
}
.storage-ex {
  font-size: 10.5px;
  color: var(--text-3);
  line-height: 1.5;
}
.storage-note {
  padding: 3px 10px;
  border-radius: 999px;
  background: var(--accent-soft);
  color: var(--accent);
  font-size: 11px;
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
  transform: translateY(10px) scale(0.96);
  filter: blur(6px);
}
.swap-leave-to {
  opacity: 0;
  transform: translateY(-10px) scale(0.96);
  filter: blur(6px);
}
.nocloud {
  position: absolute;
  right: 3%;
  top: 6%;
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 14px;
  border-radius: 999px;
  border: 1px dashed var(--border-strong);
  font-size: 13px;
  color: var(--text-3);
}
.nocloud span {
  text-decoration: line-through;
  text-decoration-color: var(--danger);
}
.nocloud b {
  color: var(--danger);
  font-weight: 600;
  font-size: 12px;
}
@media (max-width: 720px) {
  .tabs {
    display: grid;
    width: 100%;
  }
  .tab {
    padding: 8px 6px;
    font-size: 12px;
  }
  .tab svg {
    display: none;
  }
  .diagram {
    aspect-ratio: 800 / 560;
  }
  .device-icon {
    width: 44px;
    height: 44px;
    border-radius: 14px;
  }
  .device-sub,
  .storage-ex {
    display: none;
  }
  .storage {
    width: 38%;
  }
  .storage-inner {
    padding: 12px 8px;
  }
  .nocloud {
    top: 0;
    right: 50%;
    transform: translateX(50%);
    font-size: 11px;
    white-space: nowrap;
  }
}
</style>
