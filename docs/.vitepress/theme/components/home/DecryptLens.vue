<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script setup lang="ts">
/**
 * "Ciphertext until you unlock it": a vault table drawn twice — sealed records underneath and
 * the plaintext on top, visible only inside a lens that follows the pointer (or drifts on its own
 * when nobody is pointing). Touching or clicking unlocks the whole vault for a moment.
 */
import { KeyRound, Lock, LockOpen } from '@lucide/vue';
import { onBeforeUnmount, onMounted, ref } from 'vue';

import { prefersReducedMotion, useInView } from '../../composables/motion';

interface Row {
  label: string;
  target: string;
  auth: string;
  secret: string;
}
const ROWS: Row[] = [
  {
    label: 'api-prod-eu-1',
    target: 'deploy@10.20.1.11:22',
    auth: 'ed25519 · SHA256:3kqZ…v9Ue',
    secret: 'jump: bastion',
  },
  {
    label: 'db-prod-eu-1',
    target: 'deploy@10.20.2.21:22',
    auth: 'FIDO2 sk-ed25519',
    secret: 'tunnel: 5432 → 5432',
  },
  {
    label: 'raspberry-pi',
    target: 'pi@192.168.1.42:22',
    auth: 'password ••••••••',
    secret: 'mosh, keep-alive 30s',
  },
  {
    label: 'router',
    target: 'admin@192.168.1.1:23',
    auth: 'telnet · legacy opt-in',
    secret: 'snippet: show ip route',
  },
  {
    label: 'nas',
    target: 'deploy@192.168.1.10:22',
    auth: 'Secure Enclave key',
    secret: 'sftp: /volume1/backups',
  },
];

/** Deterministic pseudo-random base64, so server and client render the same ciphertext. */
function sealed(text: string, salt: number) {
  const A = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/';
  let h = 2166136261 ^ salt;
  return [...text]
    .map((c, i) => {
      if (c === ' ') return ' ';
      h = Math.imul(h ^ (c.charCodeAt(0) + i), 16777619) >>> 0;
      return A[h % 64];
    })
    .join('');
}
const SEALED = ROWS.map((r, i) => ({
  label: sealed(r.label, i * 7 + 1),
  target: sealed(r.target, i * 7 + 2),
  auth: sealed(r.auth, i * 7 + 3),
  secret: sealed(r.secret, i * 7 + 4),
}));

const box = ref<HTMLElement | null>(null);
const inView = useInView(box);
const lens = ref({ x: 50, y: 50 });
const pointing = ref(false);
const unlocked = ref(false);
let raf = 0;
let unlockTimer = 0;

function onMove(e: PointerEvent) {
  if (e.pointerType === 'touch') return;
  const r = box.value!.getBoundingClientRect();
  lens.value = {
    x: ((e.clientX - r.left) / r.width) * 100,
    y: ((e.clientY - r.top) / r.height) * 100,
  };
  pointing.value = true;
}
function onLeave() {
  pointing.value = false;
}
function unlock() {
  unlocked.value = true;
  clearTimeout(unlockTimer);
  unlockTimer = window.setTimeout(() => (unlocked.value = false), 2600);
}

function drift(now: number) {
  if (!pointing.value && inView.value) {
    const t = now / 1000;
    lens.value = { x: 50 + Math.sin(t * 0.55) * 34, y: 50 + Math.sin(t * 0.9 + 1) * 28 };
  }
  raf = requestAnimationFrame(drift);
}
onMounted(() => {
  if (!prefersReducedMotion()) raf = requestAnimationFrame(drift);
});
onBeforeUnmount(() => {
  cancelAnimationFrame(raf);
  clearTimeout(unlockTimer);
});
</script>

<template>
  <section class="lens-section">
    <div class="container grid">
      <div class="copy">
        <p class="eyebrow" v-reveal>The vault</p>
        <h2 class="h-section" v-reveal="80">
          Ciphertext, <span class="serif grad-text">until you say so.</span>
        </h2>
        <p class="lede" v-reveal="160">
          Hosts, keys, passwords and snippets are sealed record by record with XChaCha20-Poly1305.
          The vault key is wrapped by Argon2id and your OS key store — and it never reaches the
          interface you are looking at.
        </p>
        <dl class="specs" v-reveal="240">
          <div>
            <dt>Cipher</dt>
            <dd class="mono">XChaCha20-Poly1305</dd>
          </div>
          <div>
            <dt>Key derivation</dt>
            <dd class="mono">Argon2id</dd>
          </div>
          <div>
            <dt>Key storage</dt>
            <dd class="mono">Keychain · TPM · Keystore</dd>
          </div>
          <div>
            <dt>Secrets in the UI</dt>
            <dd class="mono">0 bytes</dd>
          </div>
        </dl>
      </div>

      <div
        ref="box"
        v-reveal="120"
        class="vault"
        :class="{ unlocked, pointing }"
        :style="{ '--lx': `${lens.x}%`, '--ly': `${lens.y}%` }"
        role="img"
        aria-label="A vault table: sealed records that turn into readable hosts under a moving lens"
        @pointermove="onMove"
        @pointerleave="onLeave"
        @click="unlock"
      >
        <div class="vault-head">
          <span class="status">
            <component :is="unlocked ? LockOpen : Lock" :size="14" />
            {{ unlocked ? 'Unlocked for a moment' : 'Locked · tap to unlock' }}
          </span>
          <span class="mono file">personal.vault</span>
        </div>
        <div class="layers" aria-hidden="true">
          <div class="layer sealed mono">
            <div v-for="(r, i) in SEALED" :key="i" class="row">
              <span>{{ r.label }}</span><span>{{ r.target }}</span
              ><span>{{ r.auth }}</span
              ><span>{{ r.secret }}</span>
            </div>
          </div>
          <div class="layer plain mono">
            <div v-for="r in ROWS" :key="r.label" class="row">
              <span class="host">{{ r.label }}</span><span>{{ r.target }}</span
              ><span class="auth">{{ r.auth }}</span
              ><span class="dim">{{ r.secret }}</span>
            </div>
          </div>
          <div class="lens-ring">
            <span class="lens-tag mono"><KeyRound :size="12" /> master key</span>
          </div>
        </div>
      </div>
    </div>
  </section>
</template>

<style scoped>
.lens-section {
  padding: 140px 0 80px;
}
.grid {
  display: grid;
  grid-template-columns: minmax(0, 0.9fr) minmax(0, 1.3fr);
  gap: clamp(32px, 5vw, 80px);
  align-items: center;
}
.copy {
  display: grid;
  gap: 20px;
  justify-items: start;
}
.specs {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 1px;
  width: 100%;
  margin: 8px 0 0;
  border-radius: var(--radius);
  overflow: hidden;
  border: 1px solid var(--border);
  background: var(--border);
}
.specs div {
  padding: 14px 16px;
  background: var(--bg);
}
.specs dt {
  font-size: 12px;
  color: var(--text-3);
}
.specs dd {
  margin: 2px 0 0;
  font-size: 13.5px;
  color: var(--text);
}

.vault {
  --r: 130px;
  position: relative;
  border-radius: var(--radius-lg);
  border: 1px solid var(--border);
  background: linear-gradient(180deg, var(--bg-elev), var(--bg-soft));
  box-shadow: var(--shadow-lg);
  overflow: hidden;
  cursor: none;
  user-select: none;
}
.vault-head {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 14px 18px;
  border-bottom: 1px solid var(--border);
  font-size: 12.5px;
}
.status {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  color: var(--text-2);
}
.unlocked .status {
  color: var(--ok);
}
.file {
  color: var(--text-3);
}
.layers {
  position: relative;
  padding: 10px 0;
}
.layer {
  display: grid;
  font-size: clamp(10.5px, 1vw, 13px);
}
.row {
  display: grid;
  grid-template-columns: 1fr 1.3fr 1.3fr 1.2fr;
  gap: 16px;
  padding: 14px 18px;
  border-bottom: 1px dashed var(--border);
  white-space: nowrap;
}
.row:last-child {
  border-bottom: 0;
}
.row span {
  overflow: hidden;
  text-overflow: clip;
}
.sealed {
  color: var(--text-3);
  opacity: 0.55;
  letter-spacing: 0.02em;
}
.plain {
  position: absolute;
  inset: 10px 0;
  color: var(--text);
  background:
    linear-gradient(90deg, rgb(var(--glow-1) / 0.08), rgb(var(--glow-2) / 0.06)), var(--bg-elev);
  -webkit-mask-image: radial-gradient(
    circle var(--r) at var(--lx) var(--ly),
    #000 70%,
    transparent 100%
  );
  mask-image: radial-gradient(circle var(--r) at var(--lx) var(--ly), #000 70%, transparent 100%);
  transition: -webkit-mask-size 0.6s;
}
.unlocked .plain {
  -webkit-mask-image: none;
  mask-image: none;
  animation: flash 0.6s var(--ease-out);
}
@keyframes flash {
  from {
    filter: brightness(1.8) blur(2px);
  }
}
.host {
  color: var(--accent-strong);
  font-weight: 600;
}
.auth {
  color: var(--ok);
}
.dim {
  color: var(--text-2);
}
.lens-ring {
  position: absolute;
  left: var(--lx);
  top: var(--ly);
  width: calc(var(--r) * 2);
  height: calc(var(--r) * 2);
  margin: calc(var(--r) * -1) 0 0 calc(var(--r) * -1);
  border-radius: 50%;
  border: 1px solid color-mix(in oklab, var(--accent) 60%, transparent);
  box-shadow:
    0 0 40px rgb(var(--glow-1) / 0.35),
    inset 0 0 40px rgb(var(--glow-1) / 0.15);
  pointer-events: none;
  transition: opacity 0.4s;
}
.lens-ring::before {
  content: '';
  position: absolute;
  inset: -6px;
  border-radius: 50%;
  border: 1px dashed color-mix(in oklab, var(--accent-2) 50%, transparent);
  animation: spin 18s linear infinite;
}
@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}
.unlocked .lens-ring {
  opacity: 0;
}
.lens-tag {
  position: absolute;
  left: 50%;
  bottom: -12px;
  transform: translateX(-50%);
  display: inline-flex;
  align-items: center;
  gap: 5px;
  padding: 3px 9px;
  border-radius: 999px;
  background: var(--accent);
  color: var(--on-accent);
  font-size: 10.5px;
  font-weight: 600;
  white-space: nowrap;
}
@media (max-width: 960px) {
  .grid {
    grid-template-columns: 1fr;
  }
  .vault {
    --r: 90px;
  }
  .row {
    grid-template-columns: 1fr 1.4fr;
  }
  .row span:nth-child(n + 3) {
    display: none;
  }
}
</style>
