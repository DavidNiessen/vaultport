<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script setup lang="ts">
/**
 * Hero: WebGL aurora, a headline whose last line a key decrypts once, a download button for the
 * visitor's platform, and the real app as a layered scene. The screenshot is split into depth
 * layers (window, side panel, a snippet card) with status chips pinned to the parts they talk
 * about; the whole scene tilts towards the pointer, so every layer moves by its own depth while
 * staying anchored to the same spot of the app.
 */
import {
  ArrowRight,
  BookOpen,
  Download,
  Lock,
  ShieldCheck,
  Sparkles,
  Waypoints,
} from '@lucide/vue';
import { withBase } from 'vitepress';
import { computed, onBeforeUnmount, onMounted, ref } from 'vue';

import { REPO_URL } from '../../../site';
import { prefersReducedMotion } from '../../composables/motion';
import { OS_LABEL, usePlatform } from '../../composables/platform';
import KeyReveal from './KeyReveal.vue';
import ShaderBackdrop from './ShaderBackdrop.vue';

const platform = usePlatform();
const downloadLabel = computed(() => {
  const os = platform.value?.os;
  return os ? `Download for ${OS_LABEL[os]}` : 'Download Vaultport';
});

const SHOT = '/screens/terminal.webp';

/**
 * Regions of the 2000×1250 screenshot lifted into their own layer, in fractions of the image.
 * The background is sized and positioned so the slice shows exactly the pixels underneath it.
 */
function slice(x: number, y: number, w: number, h: number) {
  return {
    left: `${x * 100}%`,
    top: `${y * 100}%`,
    width: `${w * 100}%`,
    height: `${h * 100}%`,
    backgroundImage: `url(${withBase(SHOT)})`,
    backgroundSize: `${100 / w}% ${100 / h}%`,
    backgroundPosition: `${w < 1 ? (x / (1 - w)) * 100 : 0}% ${h < 1 ? (y / (1 - h)) * 100 : 0}%`,
  };
}
const layers = [
  { cls: 'panel', style: slice(1557 / 2000, 66 / 1250, 443 / 2000, 1184 / 1250) },
  { cls: 'tabs', style: slice(0, 0, 1, 66 / 1250) },
  { cls: 'card', style: slice(1573 / 2000, 200 / 1250, 410 / 2000, 153 / 1250) },
];

/** Chips pinned to the part of the app they describe, at increasing depth. */
const chips = [
  { icon: Lock, title: 'Vault unlocked', sub: 'XChaCha20-Poly1305', cls: 'c1' },
  { icon: ShieldCheck, title: 'Host key verified', sub: 'mlkem768x25519-sha256', cls: 'c2' },
  { icon: Sparkles, title: 'Agent request approved', sub: 'Claude Code · run_command', cls: 'c3' },
  { icon: Waypoints, title: 'Synced via Syncthing', sub: 'ciphertext only', cls: 'c4' },
];

/* Pointer tilt: the scene leans towards the pointer; depth does the parallax. */
const tilt = ref({ x: 0, y: 0 });
let frame = 0;
function onPointer(e: PointerEvent) {
  if (e.pointerType !== 'mouse' || frame) return;
  const { clientX, clientY } = e;
  frame = requestAnimationFrame(() => {
    frame = 0;
    tilt.value = {
      x: clientX / window.innerWidth - 0.5,
      y: clientY / window.innerHeight - 0.5,
    };
  });
}
const sceneStyle = computed(() => ({
  transform: `rotateX(${(-tilt.value.y * 7).toFixed(2)}deg) rotateY(${(tilt.value.x * 9).toFixed(2)}deg)`,
}));
onMounted(() => {
  if (!prefersReducedMotion()) window.addEventListener('pointermove', onPointer, { passive: true });
});
onBeforeUnmount(() => {
  window.removeEventListener('pointermove', onPointer);
  cancelAnimationFrame(frame);
});

const words = ['Your', 'servers.', 'Your', 'keys.'];
</script>

<template>
  <section class="hero">
    <ShaderBackdrop />
    <div class="grid-floor" aria-hidden="true" />

    <div class="hero-copy container">
      <h1 class="display headline">
        <span class="line">
          <span v-for="(w, i) in words" :key="i" class="word" :style="{ '--i': i }"
            >{{ w }}&nbsp;</span
          >
        </span>
        <span class="line serif accent-line">
          <KeyReveal text="Nobody else’s." :delay="700" :duration="1300" />
        </span>
      </h1>

      <p class="lede sub" v-reveal="300">
        A serverless SSH, SFTP, Mosh, Telnet and serial client for every device. An end-to-end
        encrypted vault, no account, no telemetry — and sync only through storage you own.
      </p>

      <div class="ctas" v-reveal="420">
        <a v-magnetic="0.25" :href="withBase('/download')" class="btn btn-primary btn-lg">
          <Download aria-hidden="true" />
          <span>{{ downloadLabel }}</span>
        </a>
        <a v-magnetic="0.25" :href="withBase('/guide/')" class="btn btn-lg">
          <BookOpen aria-hidden="true" />
          Read the docs
          <ArrowRight class="btn-arrow" aria-hidden="true" />
        </a>
      </div>
      <p class="platforms" v-reveal="520">
        Windows, macOS, Linux, iOS and Android.
        <a :href="REPO_URL" target="_blank" rel="noreferrer">Open source under GPL-3.0.</a>
      </p>
    </div>

    <div class="stage-wrap">
      <div class="scene">
        <div class="stage" :style="sceneStyle">
          <div class="halo" aria-hidden="true" />
          <div class="shot">
            <img
              :src="withBase(SHOT)"
              alt="Vaultport with a terminal session in the see-through Pulse theme and the snippets side panel open"
              width="2000"
              height="1250"
              fetchpriority="high"
            />
            <div
              v-for="l in layers"
              :key="l.cls"
              class="layer"
              :class="l.cls"
              :style="l.style"
              aria-hidden="true"
            />
          </div>
          <div v-for="c in chips" :key="c.title" class="chip" :class="c.cls" aria-hidden="true">
            <span class="chip-icon"><component :is="c.icon" :size="16" /></span>
            <span class="chip-text">
              <strong>{{ c.title }}</strong>
              <span class="mono">{{ c.sub }}</span>
            </span>
          </div>
        </div>
      </div>
    </div>
  </section>
</template>

<style scoped>
.hero {
  position: relative;
  padding-top: calc(var(--nav-h) + 72px);
  isolation: isolate;
}
.hero :deep(.backdrop) {
  z-index: -2;
  height: 115vh;
}
.grid-floor {
  position: absolute;
  inset: 0 0 auto;
  height: 110vh;
  z-index: -1;
  background-image:
    linear-gradient(var(--border) 1px, transparent 1px),
    linear-gradient(90deg, var(--border) 1px, transparent 1px);
  background-size: 64px 64px;
  mask-image: radial-gradient(60% 50% at 50% 30%, #000, transparent 75%);
  -webkit-mask-image: radial-gradient(60% 50% at 50% 30%, #000, transparent 75%);
  opacity: 0.5;
}
.hero-copy {
  display: flex;
  flex-direction: column;
  align-items: center;
  text-align: center;
}
.headline {
  margin-top: 0;
  font-size: clamp(48px, 8.6vw, 128px);
  line-height: 0.95;
}
.line {
  display: block;
}
.word {
  display: inline-block;
  animation: word-in 0.9s var(--ease-out) both;
  animation-delay: calc(var(--i) * 70ms + 80ms);
}
@keyframes word-in {
  from {
    opacity: 0;
    transform: translateY(0.25em);
  }
}
.accent-line {
  font-size: 1.08em;
  line-height: 1.05;
  padding: 0 0.08em 0.1em;
}
.sub {
  max-width: 640px;
  margin-top: 26px;
}
.ctas {
  display: flex;
  flex-wrap: wrap;
  justify-content: center;
  gap: 12px;
  margin-top: 36px;
}
.platforms {
  margin-top: 20px;
  font-size: 14px;
  color: var(--text-3);
}
.platforms a {
  color: var(--text-2);
  text-decoration: underline;
  text-decoration-color: var(--border-strong);
  text-underline-offset: 3px;
  transition: color 0.2s;
}
.platforms a:hover {
  color: var(--text);
}

.stage-wrap {
  position: relative;
  max-width: 1180px;
  margin: 72px auto 0;
  padding: 0 var(--gutter);
  animation: stage-in 1.2s var(--ease-out) 0.35s both;
}
@keyframes stage-in {
  from {
    opacity: 0;
    transform: translateY(40px);
  }
}
.scene {
  perspective: 2200px;
}
.stage {
  position: relative;
  transform-style: preserve-3d;
  transition: transform 0.9s var(--ease-out);
}
.halo {
  position: absolute;
  inset: 6% 4% -4%;
  border-radius: 40px;
  background: conic-gradient(
    from 180deg at 50% 50%,
    rgb(var(--glow-1)),
    rgb(var(--glow-2)),
    rgb(var(--glow-3)),
    rgb(var(--glow-1))
  );
  filter: blur(70px);
  opacity: 0.45;
  transform: translateZ(-160px) scale(1.12);
}
.shot {
  position: relative;
  transform-style: preserve-3d;
  border-radius: 14px;
  box-shadow:
    0 0 0 1px rgb(255 255 255 / 0.1),
    var(--shadow-lg);
}
.shot img {
  display: block;
  width: 100%;
  height: auto;
  border-radius: 14px;
  background: #0d0c12;
}
/* Slices of the same screenshot, lifted off the window. */
.layer {
  position: absolute;
  background-repeat: no-repeat;
  pointer-events: none;
}
.panel {
  border-bottom-right-radius: 14px;
  box-shadow: -18px 0 40px -12px rgb(0 0 0 / 0.55);
  transform: translateZ(28px);
}
.tabs {
  border-radius: 14px 14px 0 0;
  box-shadow: 0 14px 30px -14px rgb(0 0 0 / 0.6);
  transform: translateZ(44px);
}
.card {
  border-radius: 12px;
  box-shadow:
    0 0 0 1px rgb(255 255 255 / 0.08),
    0 24px 48px -12px rgb(0 0 0 / 0.6);
  transform: translateZ(70px);
}

.chip {
  position: absolute;
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 10px 14px 10px 10px;
  border-radius: 14px;
  border: 1px solid var(--border);
  background: color-mix(in oklab, var(--bg-elev) 92%, transparent);
  box-shadow: var(--shadow-md);
  font-size: 13px;
  white-space: nowrap;
}
.chip-icon {
  display: grid;
  place-items: center;
  width: 32px;
  height: 32px;
  border-radius: 10px;
  background: var(--accent-soft);
  color: var(--accent);
}
.chip-text {
  display: grid;
  line-height: 1.3;
}
.chip-text strong {
  font-weight: 600;
  color: var(--text);
}
.chip-text .mono {
  font-size: 12px;
  color: var(--text-3);
}
/* Each chip sits next to what it describes. */
.c1 {
  /* the welcome banner: the vault was unlocked to connect */
  left: -3%;
  top: 14%;
  transform: translateZ(90px);
}
.c2 {
  /* the "router" tab */
  left: 23%;
  top: -3%;
  transform: translateZ(120px);
}
.c3 {
  /* below the snippets */
  right: -3%;
  top: 56%;
  transform: translateZ(110px);
}
.c4 {
  left: 6%;
  bottom: 9%;
  transform: translateZ(80px);
}
.c3 .chip-icon {
  color: rgb(var(--glow-3));
  background: rgb(var(--glow-3) / 0.14);
}
.c4 .chip-icon {
  color: var(--accent-2);
  background: rgb(var(--glow-2) / 0.16);
}
@media (max-width: 900px) {
  .chip {
    display: none;
  }
  .stage-wrap {
    margin-top: 56px;
  }
  .shot,
  .shot img {
    border-radius: 10px;
  }
  .tabs {
    border-radius: 10px 10px 0 0;
  }
  .panel {
    border-bottom-right-radius: 10px;
  }
}
@media (prefers-reduced-motion: reduce) {
  .stage-wrap {
    animation: none;
  }
}
</style>
