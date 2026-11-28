<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script setup lang="ts">
/**
 * A live XChaCha20-Poly1305 playground — the cipher that seals every vault record — running in
 * the visitor's browser with @noble/ciphers. Typing re-encrypts with a fresh nonce; "Tamper"
 * flips one bit to show the Poly1305 tag rejecting it. Nothing leaves the page.
 */
import { xchacha20poly1305 } from '@noble/ciphers/chacha.js';
import { bytesToHex, bytesToUtf8, randomBytes, utf8ToBytes } from '@noble/ciphers/utils.js';
import { Check, Dices, ShieldAlert, Unlock, Zap } from '@lucide/vue';
import { computed, onMounted, ref, watch } from 'vue';

import { prefersReducedMotion } from '../../composables/motion';

const plaintext = ref('ssh deploy@10.20.1.11 -i ~/.ssh/id_ed25519');
const key = ref<Uint8Array | null>(null);
const nonce = ref<Uint8Array | null>(null);
const sealed = ref<Uint8Array | null>(null);
/** The hex shown, with characters still "rolling" right after a change. */
const shown = ref('');
const result = ref<{ kind: 'ok'; text: string } | { kind: 'fail' } | null>(null);
const tampered = ref(false);

function encrypt() {
  if (!key.value) return;
  nonce.value = randomBytes(24);
  sealed.value = xchacha20poly1305(key.value, nonce.value).encrypt(utf8ToBytes(plaintext.value));
  tampered.value = false;
  result.value = null;
}

let raf = 0;
function roll(target: string) {
  cancelAnimationFrame(raf);
  if (prefersReducedMotion()) {
    shown.value = target;
    return;
  }
  const HEX = '0123456789abcdef';
  const start = performance.now();
  const step = (now: number) => {
    const t = (now - start) / 450;
    shown.value = [...target]
      .map((c, i) => (t > (i / target.length) * 0.6 + 0.4 ? c : HEX[(Math.random() * 16) | 0]))
      .join('');
    if (t < 1) raf = requestAnimationFrame(step);
    else shown.value = target;
  };
  raf = requestAnimationFrame(step);
}

const cipherHex = computed(() => (sealed.value ? bytesToHex(sealed.value) : ''));
watch(cipherHex, (h) => roll(h));
watch(plaintext, encrypt);

function newKey() {
  key.value = randomBytes(32);
  encrypt();
}
function decrypt() {
  if (!key.value || !nonce.value || !sealed.value) return;
  try {
    const out = xchacha20poly1305(key.value, nonce.value).decrypt(sealed.value);
    result.value = { kind: 'ok', text: bytesToUtf8(out) };
  } catch {
    result.value = { kind: 'fail' };
  }
}
function tamper() {
  if (!sealed.value) return;
  const copy = sealed.value.slice();
  const i = Math.floor(Math.random() * Math.max(1, copy.length - 16));
  copy[i] = (copy[i] ?? 0) ^ 0x01;
  sealed.value = copy;
  tampered.value = true;
  decrypt();
}

onMounted(newKey);

const sizes = computed(() => {
  const inBytes = utf8ToBytes(plaintext.value).length;
  return { inBytes, outBytes: inBytes + 16 };
});
const tagHex = computed(() => shown.value.slice(-32));
const bodyHex = computed(() => shown.value.slice(0, -32));
</script>

<template>
  <section class="play">
    <div class="container grid">
      <div class="copy">
        <p class="eyebrow" v-reveal>Try the cipher</p>
        <h2 class="h-section" v-reveal="80">
          Seal something. <span class="serif grad-text">Then break it.</span>
        </h2>
        <p class="lede" v-reveal="160">
          This is XChaCha20-Poly1305 — the same authenticated cipher that seals every record in your
          vault — running right here in your browser. Type to re-encrypt, then flip a single bit and
          watch the authentication tag refuse it.
        </p>
        <p class="note" v-reveal="220">Nothing you type leaves this page.</p>
      </div>

      <div class="console" v-reveal="120" v-spotlight>
        <label class="field">
          <span class="label"
            >Plaintext <em class="mono">{{ sizes.inBytes }} bytes</em></span
          >
          <textarea v-model="plaintext" rows="2" spellcheck="false" maxlength="160" />
        </label>

        <div class="kv">
          <div>
            <span class="label">Key <em class="mono">32 bytes · random</em></span>
            <p class="hex mono key">{{ key ? bytesToHex(key) : '' }}</p>
          </div>
          <div>
            <span class="label">Nonce <em class="mono">24 bytes · fresh every time</em></span>
            <p class="hex mono">{{ nonce ? bytesToHex(nonce) : '' }}</p>
          </div>
        </div>

        <div class="out" :class="{ tampered }">
          <span class="label"
            >Ciphertext + tag <em class="mono">{{ sizes.outBytes }} bytes</em></span
          >
          <p class="hex mono cipher">
            <span>{{ bodyHex }}</span><span class="tag">{{ tagHex }}</span>
          </p>
          <span class="legend mono"><i /> Poly1305 tag (16 bytes)</span>
        </div>

        <div class="actions">
          <button type="button" class="btn act" @click="decrypt">
            <Unlock :size="16" /> Decrypt
          </button>
          <button type="button" class="btn act danger" @click="tamper">
            <Zap :size="16" /> Tamper one bit
          </button>
          <button type="button" class="btn act ghost" @click="newKey">
            <Dices :size="16" /> New key
          </button>
        </div>

        <Transition name="res" mode="out-in">
          <p v-if="result?.kind === 'ok'" key="ok" class="result ok">
            <Check :size="16" /> Authentic. Decrypted: <span class="mono">{{ result.text }}</span>
          </p>
          <p v-else-if="result?.kind === 'fail'" key="fail" class="result fail">
            <ShieldAlert :size="16" /> Rejected: the tag does not match. Tampering detected, nothing
            was decrypted.
          </p>
        </Transition>
      </div>
    </div>
  </section>
</template>

<style scoped>
.play {
  padding: 120px 0 60px;
}
.grid {
  display: grid;
  grid-template-columns: minmax(0, 0.85fr) minmax(0, 1.2fr);
  gap: clamp(32px, 5vw, 80px);
  align-items: center;
}
.copy {
  display: grid;
  gap: 20px;
}
.note {
  font-size: 13.5px;
  color: var(--text-3);
}
.console {
  display: grid;
  gap: 18px;
  padding: 24px;
  border-radius: var(--radius-lg);
  border: 1px solid var(--border);
  background: linear-gradient(180deg, var(--bg-elev), var(--bg-soft));
  box-shadow: var(--shadow-lg);
}
.label {
  display: flex;
  justify-content: space-between;
  gap: 12px;
  font-size: 12px;
  font-weight: 600;
  letter-spacing: 0.06em;
  text-transform: uppercase;
  color: var(--text-3);
}
.label em {
  font-style: normal;
  text-transform: none;
  letter-spacing: 0;
  font-weight: 400;
}
.field {
  display: grid;
  gap: 8px;
}
textarea {
  width: 100%;
  resize: none;
  padding: 12px 14px;
  border-radius: 12px;
  border: 1px solid var(--border);
  background: var(--surface);
  font-family: var(--font-mono);
  font-size: 14px;
  line-height: 1.5;
  color: var(--text);
  outline: none;
  transition:
    border-color 0.2s,
    box-shadow 0.2s;
}
textarea:focus {
  border-color: var(--accent);
  box-shadow: 0 0 0 4px var(--accent-soft);
}
.kv {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 16px;
}
.hex {
  margin-top: 6px;
  font-size: 12px;
  line-height: 1.6;
  word-break: break-all;
  color: var(--text-2);
}
.key {
  color: var(--accent-2);
}
.out .hex {
  min-height: 5em;
  padding: 12px 14px;
  border-radius: 12px;
  background: var(--code-bg);
  border: 1px solid var(--border);
  color: var(--text);
  transition:
    border-color 0.3s,
    background-color 0.3s;
}
.tag {
  color: var(--accent);
  background: var(--accent-soft);
  border-radius: 4px;
}
.tampered .hex {
  border-color: color-mix(in oklab, var(--danger) 60%, transparent);
  animation: shake 0.4s var(--ease-out);
}
@keyframes shake {
  25% {
    transform: translateX(-5px);
  }
  50% {
    transform: translateX(4px);
  }
  75% {
    transform: translateX(-2px);
  }
}
.legend {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  margin-top: 8px;
  font-size: 12px;
  color: var(--text-3);
}
.legend i {
  width: 10px;
  height: 10px;
  border-radius: 3px;
  background: var(--accent-soft);
  border: 1px solid var(--accent);
}
.actions {
  display: flex;
  flex-wrap: wrap;
  gap: 10px;
}
.act {
  height: 42px;
  padding: 0 16px;
  font-size: 14px;
}
.act.danger:hover {
  border-color: var(--danger);
  color: var(--danger);
}
.act.ghost {
  background: transparent;
}
.result {
  display: flex;
  align-items: flex-start;
  gap: 10px;
  padding: 12px 14px;
  border-radius: 12px;
  font-size: 14px;
  line-height: 1.5;
}
.result svg {
  flex: none;
  margin-top: 2px;
}
.result.ok {
  background: color-mix(in oklab, var(--ok) 12%, transparent);
  color: var(--ok);
}
.result.ok .mono {
  color: var(--text);
  word-break: break-all;
}
.result.fail {
  background: color-mix(in oklab, var(--danger) 12%, transparent);
  color: var(--danger);
}
.res-enter-active,
.res-leave-active {
  transition:
    opacity 0.25s,
    transform 0.35s var(--ease-spring);
}
.res-enter-from,
.res-leave-to {
  opacity: 0;
  transform: translateY(6px) scale(0.98);
}
@media (max-width: 960px) {
  .grid {
    grid-template-columns: 1fr;
  }
}
@media (max-width: 560px) {
  .kv {
    grid-template-columns: 1fr;
  }
  .console {
    padding: 16px;
  }
}
</style>
