<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script setup lang="ts">
import { Check, KeyRound, Lock } from '@lucide/vue';
import { withBase } from 'vitepress';

const sources = [
  { name: '~/.ssh/config', detail: '24 hosts' },
  { name: 'known_hosts', detail: '31 keys' },
  { name: 'PuTTY', detail: '6 sessions' },
  { name: 'AWS inventory', detail: '12 instances' },
];
const stores = ['iCloud Drive', 'Dropbox', 'Syncthing', 'WebDAV', 'S3'];
</script>

<template>
  <section id="how-it-works" class="how" aria-labelledby="how-title">
    <header v-reveal class="head">
      <p class="sc-kicker">How it works</p>
      <h2 id="how-title" class="h2 sc-display">Three steps.<br />Zero sign-ups.</h2>
      <p class="sub">
        There is no account to create and nothing in between to trust. Set up once and take your
        vault to every device.
      </p>
    </header>

    <ol class="steps">
      <li v-reveal class="step">
        <span class="n">01</span>
        <h3>Create your vault</h3>
        <p>
          Pick a master password or unlock with your device. Everything is sealed with
          XChaCha20-Poly1305 before it touches the disk.
        </p>
        <div class="visual" aria-hidden="true">
          <div class="unlock">
            <span class="badge"><Lock :size="16" /></span>
            <div>
              <strong>Unlock vault</strong>
              <code>personal</code>
            </div>
          </div>
          <div class="field">••••••••••••••</div>
          <div class="button"><KeyRound :size="14" /> Unlock</div>
        </div>
      </li>

      <li v-reveal="80" class="step">
        <span class="n">02</span>
        <h3>Bring your hosts</h3>
        <p>
          Import your SSH config, known hosts, PuTTY, MobaXterm or cloud inventories in one go.
          Credentials stay on the device.
        </p>
        <ul class="visual sources" aria-hidden="true">
          <li v-for="s in sources" :key="s.name">
            <code>{{ s.name }}</code>
            <span>{{ s.detail }}</span>
            <Check :size="14" class="ok" />
          </li>
        </ul>
      </li>

      <li v-reveal="160" class="step">
        <span class="n">03</span>
        <h3>Connect anywhere</h3>
        <p>
          Double-click a host to connect. Sync through storage you already use — it only ever sees
          ciphertext.
        </p>
        <div class="visual term" aria-hidden="true">
          <code><span class="p">$</span> ssh -J bastion deploy@prod-eu-1</code>
          <code><span class="ok">✓</span> mlkem768x25519-sha256 · host key verified</code>
          <code><span class="u">deploy@prod-eu-1</span>:~$ <span class="cursor" /></code>
          <div class="stores">
            <span v-for="s in stores" :key="s">{{ s }}</span>
          </div>
        </div>
      </li>
    </ol>

    <p v-reveal class="more">
      <a :href="withBase('/guide/quick-start')">Read the quick start →</a>
    </p>
  </section>
</template>

<style scoped>
.how {
  max-width: 1200px;
  margin: 0 auto;
  padding: 120px 24px 0;
}

.head {
  max-width: 820px;
}

.h2 {
  margin: 14px 0 0;
  color: var(--vp-c-text-1);
  font-size: clamp(34px, 5.4vw, 64px);
}

.sub {
  max-width: 620px;
  margin: 20px 0 0;
  color: var(--vp-c-text-2);
  font-size: 18px;
  font-weight: 500;
  line-height: 1.55;
}

.steps {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 16px;
  margin: 56px 0 0;
  padding: 0;
  list-style: none;
}

.step {
  display: flex;
  flex-direction: column;
  padding: 28px;
  border-radius: var(--sc-radius);
  background: var(--sc-surface);
  box-shadow: 0 0 0 1px var(--sc-border);
}

.n {
  color: var(--sc-accent-text);
  font-family: var(--sc-font-display);
  font-size: 52px;
  font-stretch: 125%;
  font-weight: 900;
  line-height: 1;
}

.step h3 {
  margin: 18px 0 8px;
  color: var(--vp-c-text-1);
  font-size: 22px;
  font-weight: 800;
  letter-spacing: -0.01em;
}

.step p {
  margin: 0;
  color: var(--vp-c-text-2);
  font-size: 15.5px;
  line-height: 1.55;
}

.visual {
  margin: 24px 0 0;
  padding: 16px;
  border-radius: 16px;
  background: var(--vp-c-bg);
  box-shadow: 0 0 0 1px var(--sc-border);
  margin-top: auto;
}

.step p + .visual {
  margin-top: 24px;
}

.unlock {
  display: flex;
  align-items: center;
  gap: 12px;
}

.unlock strong {
  display: block;
  color: var(--vp-c-text-1);
  font-size: 15px;
  font-weight: 800;
}

.unlock code,
.sources code,
.term code {
  background: none;
  padding: 0;
  font-family: var(--vp-font-family-mono);
}

.unlock code {
  color: var(--sc-accent-text);
  font-size: 12px;
}

.badge {
  display: grid;
  place-items: center;
  width: 36px;
  height: 36px;
  border-radius: 999px;
  background: rgb(var(--sc-accent-rgb) / 0.14);
  color: var(--sc-accent-text);
}

.field {
  margin-top: 14px;
  padding: 10px 14px;
  border-radius: 12px;
  box-shadow: inset 0 0 0 1.5px var(--sc-accent);
  color: var(--vp-c-text-2);
  letter-spacing: 0.15em;
}

.button {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  height: 40px;
  margin-top: 10px;
  border-radius: 999px;
  background: var(--sc-accent);
  color: var(--sc-accent-fg);
  font-size: 14px;
  font-weight: 800;
}

.sources {
  list-style: none;
  padding: 8px;
}

.sources li {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 9px 10px;
  border-radius: 10px;
}

.sources li + li {
  border-top: 1px solid var(--sc-border);
}

.sources code {
  color: var(--vp-c-text-1);
  font-size: 13px;
}

.sources span {
  margin-left: auto;
  color: var(--vp-c-text-3);
  font-size: 12.5px;
  font-weight: 700;
}

.ok {
  color: var(--sc-success);
}

.term {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.term code {
  color: var(--vp-c-text-2);
  font-size: 12.5px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.term .p {
  color: var(--sc-accent-text);
}

.term .u {
  color: var(--sc-success);
}

.cursor {
  display: inline-block;
  width: 7px;
  height: 14px;
  vertical-align: -2px;
  background: var(--sc-accent);
}

.stores {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  margin-top: 10px;
}

.stores span {
  padding: 3px 9px;
  border-radius: 999px;
  background: var(--sc-chip);
  color: var(--vp-c-text-2);
  font-size: 11.5px;
  font-weight: 700;
}

.more {
  margin: 28px 0 0;
  font-weight: 800;
}

.more a {
  color: var(--sc-accent-text);
  text-decoration: none;
}

@media (max-width: 900px) {
  .steps {
    grid-template-columns: minmax(0, 1fr);
  }
}

@media (max-width: 640px) {
  .how {
    padding-top: 88px;
  }
}
</style>
