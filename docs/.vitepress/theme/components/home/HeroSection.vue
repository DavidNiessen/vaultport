<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script setup lang="ts">
/**
 * Hero: WebGL aurora, a headline whose last line decrypts itself, a download button for the
 * visitor's platform, and the real app tilted in 3D that straightens as you scroll, with status
 * chips floating at different depths.
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
import { useScrollProgress } from '../../composables/motion';
import { OS_LABEL, usePlatform } from '../../composables/platform';
import ScrambleText from './ScrambleText.vue';
import ShaderBackdrop from './ShaderBackdrop.vue';

const platform = usePlatform();
const downloadLabel = computed(() => {
  const os = platform.value?.os;
  return os ? `Download for ${OS_LABEL[os]}` : 'Download Vaultport';
});

const hero = ref<HTMLElement | null>(null);
const stage = ref<HTMLElement | null>(null);
const progress = useScrollProgress(stage);

/** 0 while the screenshot is low in the viewport, 1 once it is centred. */
const straighten = computed(() => Math.min(1, Math.max(0, (progress.value - 0.08) / 0.32)));
const stageStyle = computed(() => {
  const s = straighten.value;
  const eased = 1 - Math.pow(1 - s, 3);
  return {
    transform: `perspective(1600px) rotateX(${(1 - eased) * 26}deg) scale(${0.88 + eased * 0.12}) translateY(${(1 - eased) * -30}px)`,
  };
});

/* Pointer parallax for the floating chips */
const px = ref(0);
const py = ref(0);
function onPointer(e: PointerEvent) {
  px.value = e.clientX / window.innerWidth - 0.5;
  py.value = e.clientY / window.innerHeight - 0.5;
}
onMounted(() => window.addEventListener('pointermove', onPointer, { passive: true }));
onBeforeUnmount(() => window.removeEventListener('pointermove', onPointer));

const chips = [
  { icon: Lock, title: 'Vault unlocked', sub: 'XChaCha20-Poly1305', cls: 'c1', depth: 40 },
  {
    icon: ShieldCheck,
    title: 'Host key verified',
    sub: 'mlkem768x25519-sha256',
    cls: 'c2',
    depth: 70,
  },
  {
    icon: Sparkles,
    title: 'Agent request approved',
    sub: 'Claude Code · run_command',
    cls: 'c3',
    depth: 55,
  },
  { icon: Waypoints, title: 'Synced via Syncthing', sub: 'ciphertext only', cls: 'c4', depth: 30 },
];

const words = ['Your', 'servers.', 'Your', 'keys.'];
</script>

<template>
  <section ref="hero" class="hero">
    <ShaderBackdrop />
    <div class="grid-floor" aria-hidden="true" />

    <div class="hero-copy container">
      <a :href="REPO_URL" target="_blank" rel="noreferrer" class="badge" v-reveal>
        <span class="badge-dot" />
        Free &amp; open source · GPL-3.0
        <ArrowRight :size="14" aria-hidden="true" class="badge-arrow" />
      </a>

      <h1 class="display headline">
        <span class="line">
          <span v-for="(w, i) in words" :key="i" class="word" :style="{ '--i': i }"
            >{{ w }}&nbsp;</span
          >
        </span>
        <span class="line serif grad-text accent-line">
          <ScrambleText text="Nobody else's." :delay="650" :duration="1400" hover />
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
      <p class="platforms mono" v-reveal="520">Windows · macOS · Linux · iOS · Android</p>
    </div>

    <div ref="stage" class="stage-wrap">
      <div class="stage" :style="stageStyle">
        <div class="halo" aria-hidden="true" />
        <div class="frame">
          <img
            :src="withBase('/screens/terminal.webp')"
            alt="Vaultport with a terminal session in the see-through Pulse theme and the snippets side panel open"
            width="2000"
            height="1250"
            fetchpriority="high"
          />
          <div class="sheen" aria-hidden="true" />
        </div>
        <div
          v-for="c in chips"
          :key="c.title"
          class="chip glass"
          :class="c.cls"
          :style="{ transform: `translate3d(${px * c.depth}px, ${py * c.depth}px, 0)` }"
          aria-hidden="true"
        >
          <span class="chip-icon"><component :is="c.icon" :size="16" /></span>
          <span class="chip-text">
            <strong>{{ c.title }}</strong>
            <span class="mono">{{ c.sub }}</span>
          </span>
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
.badge {
  display: inline-flex;
  align-items: center;
  gap: 10px;
  padding: 6px 14px 6px 10px;
  border-radius: 999px;
  font-size: 13.5px;
  font-weight: 500;
  color: var(--text-2);
  position: relative;
  background: color-mix(in oklab, var(--bg-elev) 60%, transparent);
  backdrop-filter: blur(10px);
}
/* A light beam running around the badge border */
.badge::before {
  content: '';
  position: absolute;
  inset: -1px;
  border-radius: inherit;
  padding: 1px;
  background: conic-gradient(
    from var(--beam, 0deg),
    transparent 0 70%,
    rgb(var(--glow-1)) 80%,
    rgb(var(--glow-2)) 88%,
    transparent 95%
  );
  -webkit-mask:
    linear-gradient(#000 0 0) content-box,
    linear-gradient(#000 0 0);
  -webkit-mask-composite: xor;
  mask-composite: exclude;
  animation: beam 4s linear infinite;
}
.badge::after {
  content: '';
  position: absolute;
  inset: 0;
  border-radius: inherit;
  border: 1px solid var(--border);
  pointer-events: none;
}
@property --beam {
  syntax: '<angle>';
  initial-value: 0deg;
  inherits: false;
}
@keyframes beam {
  to {
    --beam: 360deg;
  }
}
.badge:hover {
  color: var(--text);
}
.badge-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: var(--ok);
  box-shadow: 0 0 10px var(--ok);
}
.badge-arrow {
  transition: transform 0.3s var(--ease-out);
}
.badge:hover .badge-arrow {
  transform: translateX(3px);
}

.headline {
  margin-top: 28px;
  font-size: clamp(48px, 8.6vw, 128px);
  line-height: 0.95;
}
.line {
  display: block;
}
.word {
  display: inline-block;
  animation: word-in 1s var(--ease-out) both;
  animation-delay: calc(var(--i) * 90ms + 100ms);
}
@keyframes word-in {
  from {
    opacity: 0;
    transform: translateY(0.5em) rotate(4deg);
    filter: blur(12px);
  }
}
.accent-line {
  font-size: 1.08em;
  line-height: 1.05;
  padding: 0 0.08em 0.1em;
  animation: word-in 1s var(--ease-out) 0.5s both;
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
  margin-top: 18px;
  font-size: 12px;
  letter-spacing: 0.12em;
  color: var(--text-3);
}

.stage-wrap {
  position: relative;
  max-width: 1180px;
  margin: 72px auto 0;
  padding: 0 var(--gutter);
}
.stage {
  position: relative;
  transform-origin: 50% 0%;
  will-change: transform;
}
.halo {
  position: absolute;
  inset: 6% 4% -4%;
  z-index: -1;
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
  animation: halo 9s var(--ease-in-out) infinite alternate;
}
@keyframes halo {
  to {
    opacity: 0.6;
    transform: scale(1.04, 0.96);
  }
}
.frame {
  position: relative;
  border-radius: 18px;
  padding: 6px;
  background: linear-gradient(180deg, rgb(255 255 255 / 0.14), rgb(255 255 255 / 0.03));
  box-shadow: var(--shadow-lg);
  overflow: hidden;
}
.frame img {
  width: 100%;
  height: auto;
  border-radius: 13px;
  background: #0d0c12;
}
.sheen {
  position: absolute;
  inset: 0;
  border-radius: inherit;
  background: linear-gradient(
    115deg,
    transparent 35%,
    rgb(255 255 255 / 0.07) 45%,
    transparent 55%
  );
  background-size: 250% 100%;
  animation: sheen 7s var(--ease-in-out) infinite;
  pointer-events: none;
}
@keyframes sheen {
  0%,
  40% {
    background-position: 150% 0;
  }
  100% {
    background-position: -100% 0;
  }
}

.chip {
  position: absolute;
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 10px 14px 10px 10px;
  border-radius: 14px;
  box-shadow: var(--shadow-md);
  font-size: 13px;
  transition: transform 0.6s var(--ease-out);
  animation: float 6s var(--ease-in-out) infinite;
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
.c1 {
  left: -2%;
  top: 18%;
}
.c2 {
  right: -3%;
  top: 8%;
  animation-delay: -2s;
}
.c3 {
  right: -1%;
  bottom: 16%;
  animation-delay: -4s;
}
.c4 {
  left: 2%;
  bottom: 6%;
  animation-delay: -3s;
}
.c3 .chip-icon {
  color: rgb(var(--glow-3));
  background: rgb(var(--glow-3) / 0.14);
}
.c4 .chip-icon {
  color: var(--accent-2);
  background: rgb(var(--glow-2) / 0.16);
}
@keyframes float {
  50% {
    translate: 0 -10px;
  }
}
@media (max-width: 900px) {
  .chip {
    display: none;
  }
  .stage-wrap {
    margin-top: 56px;
  }
  .frame {
    border-radius: 12px;
    padding: 4px;
  }
  .frame img {
    border-radius: 9px;
  }
}
</style>
