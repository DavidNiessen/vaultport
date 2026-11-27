<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script setup lang="ts">
import {
  EyeOff,
  FileCheck,
  HardDrive,
  Laptop,
  LockKeyhole,
  ServerOff,
  Smartphone,
  UserX,
} from '@lucide/vue';
import { withBase } from 'vitepress';

const claims = [
  {
    icon: UserX,
    title: 'No account',
    text: 'Nothing to sign up for and nothing to log in to. Install, create a vault, connect.',
  },
  {
    icon: EyeOff,
    title: 'No telemetry',
    text: 'No analytics, no tracking, no crash reports. Desktop builds only look for updates when you click “Check for updates”.',
  },
  {
    icon: ServerOff,
    title: 'No servers of ours',
    text: 'There is no backend to breach. Cloud inventory and the optional AI model download talk to the provider directly from your device.',
  },
  {
    icon: LockKeyhole,
    title: 'End-to-end encrypted',
    text: 'Every record is sealed with XChaCha20-Poly1305. Private keys and passwords stay in the Rust core and never reach the UI.',
  },
  {
    icon: HardDrive,
    title: 'Storage you choose',
    text: 'Sync through a folder, WebDAV or S3 bucket you control. It only ever sees ciphertext in hash-chained, tamper-evident segments.',
  },
  {
    icon: FileCheck,
    title: 'Open & verifiable',
    text: 'GPL-3.0-or-later. Releases ship with SHA-256 checksums, Sigstore signatures, build provenance and an SBOM.',
  },
];
</script>

<template>
  <section class="privacy" aria-labelledby="privacy-title">
    <div class="panel">
      <div class="aura" aria-hidden="true" />
      <header v-reveal class="head">
        <p class="kicker">Serverless &amp; private by design</p>
        <h2 id="privacy-title" class="h2">
          Your data never leaves your devices — unless you send it to storage you own.
        </h2>
      </header>

      <div
        v-reveal="80"
        class="flow"
        role="img"
        aria-label="Two of your devices exchange encrypted vault segments through your own storage. No Vaultport server is involved."
      >
        <div class="node">
          <Laptop :size="22" aria-hidden="true" />
          <span>Laptop</span>
        </div>
        <div class="link" aria-hidden="true"><span class="packet" /></div>
        <div class="node store">
          <HardDrive :size="22" aria-hidden="true" />
          <span>Your storage</span>
          <small>ciphertext only</small>
        </div>
        <div class="link" aria-hidden="true"><span class="packet reverse" /></div>
        <div class="node">
          <Smartphone :size="22" aria-hidden="true" />
          <span>Phone</span>
        </div>
        <p class="none">
          <ServerOff :size="14" aria-hidden="true" /> Vaultport cloud: <s>does not exist</s>
        </p>
      </div>

      <ul class="claims">
        <li v-for="(c, i) in claims" :key="c.title" v-reveal="i * 50" class="claim">
          <component :is="c.icon" :size="20" class="claim-icon" aria-hidden="true" />
          <h3>{{ c.title }}</h3>
          <p>{{ c.text }}</p>
        </li>
      </ul>

      <p class="more">
        <a :href="withBase('/security/')">Read the security architecture and threat model →</a>
      </p>
    </div>
  </section>
</template>

<style scoped>
.privacy {
  max-width: 1200px;
  margin: 0 auto;
  padding: 96px 24px 24px;
}

.panel {
  position: relative;
  isolation: isolate;
  overflow: hidden;
  padding: 72px 56px 56px;
  border-radius: 28px;
  background:
    radial-gradient(80% 60% at 50% 0%, rgb(99 102 241 / 0.16), transparent 70%), var(--sc-surface);
  box-shadow: 0 0 0 1px var(--sc-border);
}

.aura {
  position: absolute;
  top: 0;
  left: 15%;
  right: 15%;
  z-index: -1;
  height: 1px;
  background: linear-gradient(90deg, transparent, #818cf8, #22d3ee, transparent);
}

.head {
  max-width: 820px;
  margin: 0 auto;
  text-align: center;
}

.kicker {
  margin: 0;
  color: var(--sc-accent);
  font-size: 13px;
  font-weight: 600;
  letter-spacing: 0.08em;
  text-transform: uppercase;
}

.h2 {
  margin: 14px 0 0;
  color: var(--vp-c-text-1);
  font-size: clamp(28px, 3.8vw, 42px);
  font-weight: 700;
  line-height: 1.15;
  letter-spacing: -0.03em;
  text-wrap: balance;
}

.flow {
  position: relative;
  display: flex;
  align-items: center;
  justify-content: center;
  flex-wrap: wrap;
  gap: 0;
  max-width: 760px;
  margin: 48px auto 0;
  padding-bottom: 36px;
}

.node {
  display: grid;
  justify-items: center;
  gap: 6px;
  width: 128px;
  padding: 18px 12px;
  border-radius: 16px;
  background: var(--vp-c-bg-soft);
  box-shadow: inset 0 0 0 1px var(--sc-border);
  color: var(--vp-c-text-1);
  font-size: 14px;
  font-weight: 600;
}

.node svg {
  color: var(--sc-accent);
}

.node.store {
  width: 156px;
  box-shadow:
    inset 0 0 0 1px rgb(99 102 241 / 0.5),
    0 0 40px -10px var(--sc-glow);
}

.node small {
  color: var(--vp-c-text-3);
  font-size: 12px;
  font-weight: 500;
}

.link {
  position: relative;
  flex: 1;
  min-width: 40px;
  max-width: 140px;
  height: 2px;
  background: repeating-linear-gradient(90deg, var(--sc-border-strong) 0 6px, transparent 6px 12px);
}

.packet {
  position: absolute;
  top: -4px;
  left: 0;
  width: 10px;
  height: 10px;
  border-radius: 3px;
  background: var(--sc-accent);
  box-shadow: 0 0 12px var(--sc-accent);
  animation: travel 2.8s var(--sc-ease) infinite;
}

.packet.reverse {
  animation-direction: reverse;
  animation-delay: 1.4s;
  background: var(--sc-cyan);
  box-shadow: 0 0 12px var(--sc-cyan);
}

@keyframes travel {
  0% {
    left: 0;
    opacity: 0;
  }
  15%,
  85% {
    opacity: 1;
  }
  100% {
    left: calc(100% - 10px);
    opacity: 0;
  }
}

.none {
  position: absolute;
  bottom: 0;
  left: 0;
  right: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  margin: 0;
  color: var(--vp-c-text-3);
  font-size: 13px;
}

.claims {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 16px;
  margin: 48px 0 0;
  padding: 0;
  list-style: none;
}

.claim {
  padding: 22px;
  border-radius: 16px;
  background: color-mix(in srgb, var(--vp-c-bg) 50%, transparent);
  box-shadow: inset 0 0 0 1px var(--sc-border);
}

.claim-icon {
  color: var(--sc-accent);
}

.claim h3 {
  margin: 12px 0 0;
  color: var(--vp-c-text-1);
  font-size: 16px;
  font-weight: 600;
}

.claim p {
  margin: 6px 0 0;
  color: var(--vp-c-text-2);
  font-size: 14px;
  line-height: 1.6;
}

.more {
  margin: 32px 0 0;
  text-align: center;
}

.more a {
  color: var(--vp-c-brand-1);
  font-size: 15px;
  font-weight: 500;
  text-decoration: none;
}

.more a:hover {
  text-decoration: underline;
}

@media (max-width: 960px) {
  .claims {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }
  .panel {
    padding: 56px 28px 40px;
  }
}

@media (max-width: 640px) {
  .privacy {
    padding: 72px 16px 16px;
  }
  .panel {
    padding: 44px 18px 28px;
    border-radius: 22px;
  }
  .claims {
    grid-template-columns: 1fr;
  }
  .node {
    width: 92px;
    padding: 14px 8px;
    font-size: 12.5px;
  }
  .node.store {
    width: 112px;
  }
  .link {
    min-width: 16px;
  }
}

@media (prefers-reduced-motion: reduce) {
  .packet {
    animation: none;
    left: calc(50% - 5px);
  }
}
</style>
