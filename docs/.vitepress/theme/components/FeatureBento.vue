<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script setup lang="ts">
import {
  AppWindow,
  ArrowLeftRight,
  Bot,
  Braces,
  CloudDownload,
  Fingerprint,
  Laptop,
  LayoutGrid,
  LockKeyhole,
  QrCode,
  ScrollText,
  Server,
  Smartphone,
  Sparkles,
  SquareTerminal,
  Waypoints,
} from '@lucide/vue';

import FeatureCard from './FeatureCard.vue';

const protocols = [
  { name: 'SSH', detail: 'Pure-Rust engine · agent forwarding · env vars' },
  { name: 'SFTP', detail: 'Dual-pane browser · resumable queue · edit & upload' },
  { name: 'Mosh', detail: 'Native client · survives roaming & sleep' },
  { name: 'Telnet', detail: 'RFC 854 · NAWS window size · for trusted networks' },
  { name: 'Serial', detail: 'USB adapters & console cables · up to 921600 baud' },
];

const forwards = [
  { kind: 'L', label: 'Local', from: '127.0.0.1:5432', to: 'db.internal:5432' },
  { kind: 'R', label: 'Remote', from: '0.0.0.0:8080', to: 'localhost:3000' },
  { kind: 'D', label: 'Dynamic', from: '127.0.0.1:1080', to: 'SOCKS5 proxy' },
];

const importers = ['OpenSSH config', 'known_hosts', 'PuTTY', 'MobaXterm', 'SecureCRT', 'CSV'];
const clouds = ['AWS EC2 & Lightsail', 'DigitalOcean', 'Azure'];

/** Data modules of a decorative, QR-like 21×21 pattern (not a real code). */
const QR_SIZE = 21;
const QR_FINDERS: [number, number][] = [
  [0, 0],
  [14, 0],
  [0, 14],
];
const QR_MODULES = (() => {
  let seed = 20261127;
  let d = '';
  for (let y = 0; y < QR_SIZE; y++) {
    for (let x = 0; x < QR_SIZE; x++) {
      seed = (seed * 1103515245 + 12345) & 0x7fffffff;
      const inFinder = QR_FINDERS.some(
        ([fx, fy]) => x >= fx - 1 && x <= fx + 7 && y >= fy - 1 && y <= fy + 7,
      );
      if (!inFinder && seed % 100 < 46) d += `M${x} ${y}h1v1h-1z`;
    }
  }
  return d;
})();
</script>

<template>
  <section id="features" class="features" aria-labelledby="features-title">
    <header v-reveal class="head">
      <p class="kicker">Everything in one app</p>
      <h2 id="features-title" class="h2">Built for real infrastructure.</h2>
      <p class="sub">
        From a single Raspberry Pi to fleets behind bastions: the protocols, tunnels and tooling you
        reach for every day, with security defaults you don't have to think about.
      </p>
    </header>

    <div class="bento">
      <!-- Protocols -->
      <FeatureCard
        class="span-7"
        :icon="SquareTerminal"
        title="SSH, SFTP, Mosh, Telnet & Serial"
        href="/guide/hosts-and-groups"
      >
        Every protocol in one tabbed window, plus a local terminal on desktop. Post-quantum
        <code>mlkem768x25519-sha256</code> key exchange is preferred, with modern ciphers only
        unless you opt in to legacy algorithms for an old box.
        <template #visual>
          <ul class="proto">
            <li v-for="p in protocols" :key="p.name">
              <span class="proto-name">{{ p.name }}</span>
              <span class="proto-detail">{{ p.detail }}</span>
              <span class="status" />
            </li>
          </ul>
        </template>
      </FeatureCard>

      <!-- Vault -->
      <FeatureCard
        class="span-5"
        :icon="LockKeyhole"
        title="End-to-end encrypted vault"
        href="/concepts/vault"
        :delay="80"
      >
        Hosts, keys, passwords and settings are sealed on your device. Unlock with a master password
        or the OS key store — nobody, including us, can recover it.
        <template #visual>
          <dl class="spec">
            <div>
              <dt>cipher</dt>
              <dd>XChaCha20-Poly1305</dd>
            </div>
            <div>
              <dt>kdf</dt>
              <dd>Argon2id · 256 MiB · t=3</dd>
            </div>
            <div>
              <dt>unlock</dt>
              <dd>password and/or OS key store</dd>
            </div>
            <div>
              <dt>storage</dt>
              <dd>CRDT op log · SQLite</dd>
            </div>
          </dl>
        </template>
      </FeatureCard>

      <!-- Jump hosts -->
      <FeatureCard
        class="span-4"
        :icon="Waypoints"
        title="Jump hosts & proxies"
        href="/guide/jump-hosts-and-proxies"
      >
        Chain any number of bastions (ProxyJump). Reach the first hop through HTTP CONNECT or SOCKS5
        — each hop gets its own host-key check and authentication.
        <template #visual>
          <div class="hops">
            <span class="hop"><Laptop :size="16" /> you</span>
            <span class="wire" />
            <span class="hop accent"><Server :size="16" /> bastion</span>
            <span class="wire" />
            <span class="hop"><Server :size="16" /> 10.0.0.5</span>
          </div>
          <p class="via">via SOCKS5 · proxy-side DNS</p>
        </template>
      </FeatureCard>

      <!-- Port forwarding -->
      <FeatureCard
        class="span-4"
        :icon="ArrowLeftRight"
        title="Port forwarding"
        href="/guide/port-forwarding"
        :delay="60"
      >
        Saved local, remote and dynamic (SOCKS) tunnels. Start them with a double-click or
        automatically when the vault unlocks.
        <template #visual>
          <ul class="fwd">
            <li v-for="f in forwards" :key="f.kind">
              <span class="badge">{{ f.kind }}</span>
              <span class="mono from">{{ f.from }}</span>
              <span class="arrow">→</span>
              <span class="mono to">{{ f.to }}</span>
            </li>
          </ul>
        </template>
      </FeatureCard>

      <!-- Hardware keys -->
      <FeatureCard
        class="span-4"
        :icon="Fingerprint"
        title="Hardware-backed keys"
        href="/guide/keys"
        :delay="120"
      >
        FIDO2 security keys (<code>sk-ssh-ed25519</code>, <code>sk-ecdsa</code>) and non-exportable
        biometric keys in the Secure Enclave or Windows Hello.
        <template #visual>
          <div class="touch">
            <span class="ring"><Fingerprint :size="30" /></span>
            <span class="touch-text">Touch your security key…</span>
          </div>
        </template>
      </FeatureCard>

      <!-- Workspaces -->
      <FeatureCard
        class="span-7"
        :icon="LayoutGrid"
        title="Workspaces & broadcast"
        href="/guide/workspaces"
      >
        Open several hosts in one tab as a split grid or in focus mode, save the layout, and
        broadcast keystrokes to every terminal at once.
        <template #visual>
          <div class="ws">
            <div class="ws-bar">
              <span class="ws-name">web · 4 hosts</span>
              <span class="broadcast"><span class="live" /> Broadcast</span>
            </div>
            <div class="ws-grid">
              <div v-for="n in 4" :key="n" class="ws-term">
                <span class="mono dim">web-0{{ n }}:~$</span>
                <span class="mono"> sudo apt upgrade</span><span class="caret" />
              </div>
            </div>
          </div>
        </template>
      </FeatureCard>

      <!-- Snippets -->
      <FeatureCard
        class="span-5"
        :icon="Braces"
        title="Snippets & autocomplete"
        href="/guide/snippets"
        :delay="80"
      >
        Reusable commands with <code v-pre>{{ variables }}</code>, run on one host or many. The
        completion engine ranks history, snippets, paths and ~60 command specs — no AI involved.
        <template #visual>
          <div class="ac">
            <div class="ac-input mono">
              <span class="dim">$</span> systemctl re<span class="caret" />
            </div>
            <ul class="ac-list">
              <li class="on"><span class="mono">restart</span><span class="src">spec</span></li>
              <li><span class="mono">reload</span><span class="src">spec</span></li>
              <li><span class="mono">reload nginx</span><span class="src">history</span></li>
            </ul>
          </div>
        </template>
      </FeatureCard>

      <!-- Sync -->
      <FeatureCard
        class="span-5"
        :icon="QrCode"
        title="Serverless sync & QR pairing"
        href="/guide/sync"
      >
        Replicate the encrypted vault through a synced folder, WebDAV or any S3-compatible bucket.
        Edits merge without conflicts. Add a device by scanning a QR code.
        <template #visual>
          <div class="sync">
            <svg class="qr" viewBox="-2 -2 25 25" aria-hidden="true">
              <rect x="-2" y="-2" width="25" height="25" rx="3" fill="#fff" />
              <path :d="QR_MODULES" fill="#111827" />
              <g v-for="[x, y] in QR_FINDERS" :key="`${x}-${y}`" fill="#111827">
                <path :d="`M${x} ${y}h7v7h-7z M${x + 1} ${y + 1}v5h5v-5z`" fill-rule="evenodd" />
                <rect :x="x + 2" :y="y + 2" width="3" height="3" />
              </g>
            </svg>
            <div class="sync-side">
              <span class="code mono">K7QM-2XPA</span>
              <span class="chips">
                <span class="chip">Folder</span>
                <span class="chip">WebDAV</span>
                <span class="chip">S3</span>
              </span>
            </div>
          </div>
        </template>
      </FeatureCard>

      <!-- Importers & cloud -->
      <FeatureCard
        class="span-7"
        :icon="CloudDownload"
        title="Importers & cloud inventory"
        href="/guide/importing"
        :delay="80"
      >
        Bring hosts, keys and known hosts from other clients in one step, or keep a group in sync
        with your cloud accounts. Provider APIs are called directly from your device with read-only
        credentials.
        <template #visual>
          <div class="import">
            <div class="chips">
              <span v-for="i in importers" :key="i" class="chip">{{ i }}</span>
            </div>
            <div class="chips">
              <span v-for="c in clouds" :key="c" class="chip cloud">{{ c }}</span>
            </div>
          </div>
        </template>
      </FeatureCard>

      <!-- Session logs -->
      <FeatureCard
        class="span-5"
        :icon="ScrollText"
        title="Encrypted session logs"
        href="/guide/session-logs"
      >
        Sessions are recorded as asciicast v2, sealed in tamper-evident chunks. Search, replay and
        export to <code>.cast</code>. Keystrokes are never recorded by default.
        <template #visual>
          <ul class="logs">
            <li><span class="lock">●</span> prod-eu-1 <span class="dim">· 14 min</span></li>
            <li><span class="lock">●</span> db-replica <span class="dim">· 3 min</span></li>
            <li><span class="lock">●</span> switch-core <span class="dim">· serial</span></li>
          </ul>
        </template>
      </FeatureCard>

      <!-- Title bars -->
      <FeatureCard
        class="span-7"
        :icon="AppWindow"
        title="Feels native everywhere"
        href="/guide/"
        :delay="80"
      >
        A custom title bar with tabs that follows each OS: traffic lights on macOS, Fluent caption
        buttons on Windows, and your GNOME button layout on Linux.
        <template #visual>
          <div class="bars">
            <div class="bar mac"><span /><span /><span /><em>macOS</em></div>
            <div class="bar win"><em>Windows</em><i>—</i><i>☐</i><i>✕</i></div>
            <div class="bar gnome"><em>Linux</em><b>−</b><b>□</b><b>×</b></div>
          </div>
        </template>
      </FeatureCard>

      <!-- AI agents -->
      <FeatureCard
        class="span-7"
        :icon="Bot"
        title="AI agents, on your terms"
        href="/guide/ai-agents"
      >
        Let Claude Code, Cursor, VS Code or Codex work with your servers over MCP. Agents never see
        your keys or passwords, and every change waits for your approval — off by default, local
        only.
        <template #visual>
          <div class="approve">
            <p class="approve-title"><Bot :size="15" /> Claude Code wants to act</p>
            <p class="approve-sub">Run command <span class="dim">on</span> prod-eu-1</p>
            <p class="approve-cmd mono">sudo systemctl reload nginx</p>
            <div class="approve-actions">
              <span class="btn-ghost">Deny</span>
              <span class="btn-ghost">Allow for this session</span>
              <span class="btn-solid">Allow once</span>
            </div>
          </div>
        </template>
      </FeatureCard>

      <!-- Local AI -->
      <FeatureCard
        class="span-5"
        :icon="Sparkles"
        title="Optional, fully local AI"
        href="/guide/local-ai"
        :delay="80"
      >
        Off by default. Turn plain language into a shell command with a small model that runs
        on-device. Nothing you type leaves your computer, and nothing runs without you.
        <template #visual>
          <div class="ai">
            <p class="ai-q">find the 10 largest files in /var/log</p>
            <p class="ai-a mono">du -ah /var/log | sort -rh | head -n 10</p>
          </div>
        </template>
      </FeatureCard>

      <!-- Mobile -->
      <FeatureCard
        class="span-12 mobile"
        :icon="Smartphone"
        title="Made for phones & tablets too"
        href="/guide/mobile"
      >
        A touch-first layout with a terminal key bar (Esc, Tab, sticky Ctrl/Alt, arrows, symbols and
        function keys), quick font-size buttons, and automatic locking when the app goes to the
        background. Same vault, same features, in your pocket.
        <template #visual>
          <div class="phone">
            <div class="screen">
              <p class="mono"><span class="ok">pi@garage</span>:~$ systemctl status nginx</p>
              <p class="mono"><span class="ok">●</span> nginx.service</p>
              <p class="mono dim">Active: active (running)</p>
              <p class="mono"><span class="ok">pi@garage</span>:~$ df -h /</p>
              <p class="mono dim">Size Used Avail Use%</p>
              <p class="mono dim">29G 8.1G 20G 29%</p>
              <p class="mono"><span class="ok">pi@garage</span>:~$ <span class="caret" /></p>
            </div>
            <div class="keys">
              <span>esc</span><span>tab</span><span class="on">ctrl</span><span>alt</span
              ><span>↑</span><span>↓</span>
            </div>
          </div>
        </template>
      </FeatureCard>
    </div>
  </section>
</template>

<style scoped>
.features {
  max-width: 1200px;
  margin: 0 auto;
  padding: 112px 24px 24px;
}

.head {
  max-width: 820px;
  margin: 0 0 56px;
}

.kicker {
  margin: 0;
  color: var(--sc-accent-text);
  font-size: 13px;
  font-weight: 800;
  letter-spacing: 0.14em;
  text-transform: uppercase;
}

.h2 {
  margin: 14px 0 0;
  color: var(--vp-c-text-1);
  font-family: var(--sc-font-display);
  font-size: clamp(34px, 5.4vw, 64px);
  font-stretch: 125%;
  font-weight: 800;
  line-height: 1;
  letter-spacing: -0.01em;
  text-transform: uppercase;
  text-wrap: balance;
}

.sub {
  max-width: 640px;
  margin: 20px 0 0;
  color: var(--vp-c-text-2);
  font-size: 18px;
  font-weight: 500;
  line-height: 1.6;
  text-wrap: pretty;
}

.bento {
  display: grid;
  grid-template-columns: repeat(12, minmax(0, 1fr));
  gap: 16px;
}

.span-4 {
  grid-column: span 4;
}
.span-5 {
  grid-column: span 5;
}
.span-7 {
  grid-column: span 7;
}
.span-12 {
  grid-column: span 12;
}

.bento :deep(code) {
  white-space: nowrap;
  padding: 1px 5px;
  border-radius: 5px;
  background: var(--vp-c-default-soft, rgb(127 127 127 / 0.12));
  font-family: var(--vp-font-family-mono);
  font-size: 0.86em;
}

.mono {
  font-family: var(--vp-font-family-mono);
}

.dim {
  color: var(--vp-c-text-3);
}

.chips {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}

.chip {
  padding: 5px 10px;
  border-radius: 999px;
  background: var(--vp-c-bg-soft);
  box-shadow: inset 0 0 0 1px var(--sc-border);
  color: var(--vp-c-text-2);
  font-size: 12.5px;
  font-weight: 500;
  white-space: nowrap;
}

.chip.cloud {
  background: var(--vp-c-brand-soft);
  box-shadow: inset 0 0 0 1px rgb(var(--sc-accent-rgb) / 0.3);
  color: var(--vp-c-brand-1);
}

.caret {
  display: inline-block;
  width: 7px;
  height: 1.1em;
  margin-left: 2px;
  vertical-align: -0.2em;
  background: var(--sc-accent);
  animation: blink 1.1s steps(1) infinite;
}

@keyframes blink {
  50% {
    opacity: 0;
  }
}

/* Protocols */
.proto {
  display: grid;
  gap: 8px;
  margin: 0;
  padding: 0;
  list-style: none;
}

.proto li {
  display: grid;
  grid-template-columns: 72px 1fr auto;
  align-items: center;
  gap: 12px;
  padding: 10px 14px;
  border-radius: 12px;
  background: var(--vp-c-bg-soft);
  box-shadow: inset 0 0 0 1px var(--sc-border);
}

.proto-name {
  color: var(--vp-c-text-1);
  font-family: var(--vp-font-family-mono);
  font-size: 13px;
  font-weight: 600;
}

.proto-detail {
  overflow: hidden;
  color: var(--vp-c-text-2);
  font-size: 13px;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.status {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: var(--sc-success);
  box-shadow: 0 0 10px var(--sc-success);
}

/* Vault spec */
.spec {
  display: grid;
  gap: 0;
  margin: 0;
  padding: 6px 16px;
  border-radius: 14px;
  background: #12181d;
  box-shadow: inset 0 0 0 1px rgb(255 255 255 / 0.07);
  font-family: var(--vp-font-family-mono);
  font-size: 12.5px;
}

.spec div {
  display: grid;
  grid-template-columns: 72px 1fr;
  gap: 12px;
  padding: 9px 0;
  border-bottom: 1px dashed rgb(255 255 255 / 0.08);
}

.spec div:last-child {
  border-bottom: 0;
}

.spec dt {
  color: #7e86a3;
}

.spec dd {
  margin: 0;
  color: var(--sc-accent-text);
}

/* Jump hosts */
.hops {
  display: flex;
  align-items: center;
  gap: 6px;
  padding-top: 8px;
}

.hop {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 8px 10px;
  border-radius: 10px;
  background: var(--vp-c-bg-soft);
  box-shadow: inset 0 0 0 1px var(--sc-border);
  color: var(--vp-c-text-1);
  font-family: var(--vp-font-family-mono);
  font-size: 12px;
  white-space: nowrap;
}

.hop.accent {
  box-shadow: inset 0 0 0 1px rgb(var(--sc-accent-rgb) / 0.5);
  color: var(--vp-c-brand-1);
}

.wire {
  position: relative;
  flex: 1;
  min-width: 12px;
  height: 2px;
  overflow: hidden;
  border-radius: 2px;
  background: var(--sc-border-strong);
}

.wire::after {
  content: '';
  position: absolute;
  inset: 0;
  width: 40%;
  background: linear-gradient(90deg, transparent, var(--sc-accent), transparent);
  animation: flow 2.2s linear infinite;
}

@keyframes flow {
  from {
    transform: translateX(-100%);
  }
  to {
    transform: translateX(260%);
  }
}

.via {
  margin: 14px 0 0;
  color: var(--vp-c-text-3);
  font-family: var(--vp-font-family-mono);
  font-size: 12px;
}

/* Forwarding */
.fwd {
  display: grid;
  gap: 8px;
  margin: 0;
  padding: 0;
  list-style: none;
  font-size: 12px;
}

.fwd li {
  display: grid;
  grid-template-columns: auto minmax(0, 1fr) auto minmax(0, 1fr);
  align-items: center;
  gap: 8px;
  padding: 8px 10px;
  border-radius: 10px;
  background: var(--vp-c-bg-soft);
  box-shadow: inset 0 0 0 1px var(--sc-border);
}

.badge {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 22px;
  height: 22px;
  border-radius: 6px;
  background: var(--vp-c-brand-soft);
  color: var(--vp-c-brand-1);
  font-family: var(--vp-font-family-mono);
  font-weight: 700;
}

.from,
.to {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.from {
  color: var(--vp-c-text-1);
}

.to {
  color: var(--vp-c-text-2);
}

.arrow {
  color: var(--vp-c-text-3);
}

/* Hardware keys */
.touch {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 14px;
  height: 100%;
  min-height: 120px;
}

.ring {
  position: relative;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 64px;
  height: 64px;
  border-radius: 50%;
  background: var(--vp-c-brand-soft);
  color: var(--sc-accent-text);
}

.ring::before,
.ring::after {
  content: '';
  position: absolute;
  inset: -6px;
  border: 1px solid rgb(var(--sc-accent-rgb) / 0.5);
  border-radius: 50%;
  animation: halo 2.6s var(--sc-ease) infinite;
}

.ring::after {
  animation-delay: 1.3s;
}

@keyframes halo {
  from {
    transform: scale(0.9);
    opacity: 1;
  }
  to {
    transform: scale(1.5);
    opacity: 0;
  }
}

.touch-text {
  color: var(--vp-c-text-2);
  font-size: 13px;
}

/* Workspaces */
.ws {
  overflow: hidden;
  border-radius: 14px;
  background: #12181d;
  box-shadow: inset 0 0 0 1px rgb(255 255 255 / 0.07);
}

.ws-bar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px 12px;
  border-bottom: 1px solid rgb(255 255 255 / 0.06);
  font-size: 12px;
}

.ws-name {
  color: #9ba2b6;
}

.broadcast {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 3px 9px;
  border-radius: 999px;
  background: rgb(245 158 11 / 0.15);
  color: #f4c15c;
  font-weight: 600;
}

.live {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: #f59e0b;
}

.ws-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 1px;
  background: rgb(255 255 255 / 0.06);
}

.ws-term {
  overflow: hidden;
  padding: 14px 12px;
  background: #12181d;
  color: #d8dcea;
  font-size: 12px;
  white-space: nowrap;
}

.ws-term .dim {
  color: #5fd69b;
}

.ws-term .caret {
  background: #8b8ff8;
}

/* Autocomplete */
.ac {
  position: relative;
}

.ac-input {
  padding: 10px 12px;
  border-radius: 10px;
  background: #12181d;
  box-shadow: inset 0 0 0 1px rgb(255 255 255 / 0.07);
  color: #d8dcea;
  font-size: 13px;
}

.ac-input .dim {
  color: #7e86a3;
}

.ac-list {
  margin: 6px 0 0 28px;
  padding: 4px;
  border-radius: 10px;
  background: var(--vp-c-bg-elv);
  box-shadow:
    0 0 0 1px var(--sc-border-strong),
    0 16px 32px -16px rgb(0 0 0 / 0.5);
  list-style: none;
  font-size: 12.5px;
}

.ac-list li {
  display: flex;
  justify-content: space-between;
  gap: 12px;
  padding: 6px 10px;
  border-radius: 7px;
  color: var(--vp-c-text-2);
}

.ac-list li.on {
  background: var(--vp-c-brand-soft);
  color: var(--vp-c-text-1);
}

.src {
  color: var(--vp-c-text-3);
  font-size: 11px;
}

/* Sync */
.sync {
  display: flex;
  align-items: center;
  gap: 20px;
}

.qr {
  flex-shrink: 0;
  width: 104px;
  height: 104px;
  border-radius: 12px;
  box-shadow: 0 12px 28px -12px rgb(0 0 0 / 0.5);
  shape-rendering: crispEdges;
}

.sync-side {
  display: grid;
  gap: 12px;
}

.code {
  color: var(--vp-c-text-1);
  font-size: 20px;
  font-weight: 600;
  letter-spacing: 0.08em;
}

/* Importers */
.import {
  display: grid;
  gap: 12px;
  align-content: center;
  height: 100%;
}

/* Logs */
.logs {
  display: grid;
  gap: 6px;
  margin: 0;
  padding: 0;
  list-style: none;
  font-size: 13px;
}

.logs li {
  padding: 8px 12px;
  border-radius: 10px;
  background: var(--vp-c-bg-soft);
  box-shadow: inset 0 0 0 1px var(--sc-border);
  color: var(--vp-c-text-1);
}

.lock {
  margin-right: 6px;
  color: var(--sc-success);
  font-size: 9px;
  vertical-align: 2px;
}

/* AI agents */
.approve {
  padding: 14px;
  border-radius: 14px;
  background: var(--vp-c-bg-elv);
  box-shadow:
    0 0 0 1px var(--sc-border-strong),
    0 18px 36px -18px rgb(0 0 0 / 0.45);
}

.approve p {
  margin: 0;
}

.approve-title {
  display: flex;
  align-items: center;
  gap: 8px;
  color: var(--vp-c-text-1);
  font-size: 14px;
  font-weight: 600;
}

.approve-title svg {
  color: var(--sc-accent-text);
}

.approve-sub {
  margin-top: 4px !important;
  color: var(--vp-c-text-2);
  font-size: 13px;
}

.approve-cmd {
  margin-top: 10px !important;
  padding: 9px 12px;
  overflow: hidden;
  border-radius: 9px;
  background: #12181d;
  box-shadow: inset 0 0 0 1px rgb(255 255 255 / 0.07);
  color: #7be2ee;
  font-size: 12.5px;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.approve-actions {
  display: flex;
  flex-wrap: wrap;
  justify-content: flex-end;
  gap: 6px;
  margin-top: 12px;
}

.approve-actions span {
  padding: 5px 10px;
  border-radius: 8px;
  font-size: 12px;
  font-weight: 500;
  white-space: nowrap;
}

.btn-ghost {
  box-shadow: inset 0 0 0 1px var(--sc-border-strong);
  color: var(--vp-c-text-2);
}

.btn-solid {
  background: var(--sc-accent);
  color: var(--sc-accent-fg);
}

/* AI */
.ai {
  display: grid;
  gap: 8px;
}

.ai-q {
  margin: 0;
  justify-self: end;
  max-width: 90%;
  padding: 8px 12px;
  border-radius: 12px 12px 4px 12px;
  background: var(--vp-c-brand-soft);
  color: var(--vp-c-text-1);
  font-size: 13px;
}

.ai-a {
  margin: 0;
  padding: 10px 12px;
  overflow: hidden;
  border-radius: 10px;
  background: #12181d;
  box-shadow: inset 0 0 0 1px rgb(255 255 255 / 0.07);
  color: #7be2ee;
  font-size: 12px;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* Title bars */
.bars {
  display: grid;
  gap: 8px;
}

.bar {
  display: flex;
  align-items: center;
  gap: 6px;
  height: 30px;
  padding: 0 10px;
  border-radius: 9px;
  background: var(--vp-c-bg-soft);
  box-shadow: inset 0 0 0 1px var(--sc-border);
  color: var(--vp-c-text-2);
  font-size: 12px;
}

.bar em {
  flex: 1;
  font-style: normal;
}

.bar.mac span {
  width: 10px;
  height: 10px;
  border-radius: 50%;
  background: #ff5f57;
}
.bar.mac span:nth-child(2) {
  background: #febc2e;
}
.bar.mac span:nth-child(3) {
  margin-right: 6px;
  background: #28c840;
}

.bar.win {
  padding-right: 0;
}

.bar.win i {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 34px;
  height: 100%;
  font-size: 11px;
  font-style: normal;
}

.bar.win i:last-child {
  border-radius: 0 9px 9px 0;
  background: #c42b1c;
  color: #fff;
}

.bar.gnome b {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 20px;
  height: 20px;
  border-radius: 50%;
  background: var(--vp-c-default-soft, rgb(127 127 127 / 0.15));
  font-size: 12px;
  font-weight: 500;
}

/* Mobile */
.mobile {
  flex-direction: row-reverse;
  align-items: center;
}

.mobile :deep(.copy) {
  flex: 1;
  max-width: 560px;
}

.mobile :deep(.visual) {
  display: flex;
  justify-content: center;
  flex: 1;
}

.phone {
  width: 210px;
  padding: 10px;
  border-radius: 30px;
  background: #0b0f12;
  box-shadow:
    0 0 0 1px rgb(255 255 255 / 0.12),
    0 0 0 6px #1a1e2b,
    0 30px 60px -20px rgb(0 0 0 / 0.6);
}

.screen {
  height: 250px;
  padding: 16px 12px;
  overflow: hidden;
  border-radius: 20px 20px 6px 6px;
  background: #12181d;
  font-size: 10.5px;
  line-height: 1.75;
}

.screen p {
  margin: 0;
  overflow: hidden;
  color: #d8dcea;
  text-overflow: ellipsis;
  white-space: pre;
}

.screen .caret {
  background: #8b8ff8;
}

.screen .dim {
  color: #7e86a3;
}

.ok {
  color: #5fd69b;
}

.keys {
  display: flex;
  gap: 4px;
  padding: 8px 2px 4px;
}

.keys span {
  display: inline-flex;
  flex: 1;
  align-items: center;
  justify-content: center;
  height: 26px;
  border-radius: 7px;
  background: #1e2331;
  color: #e8eaf2;
  font-family: var(--vp-font-family-mono);
  font-size: 10px;
}

.keys span.on {
  background: var(--sc-accent);
  color: var(--sc-accent-fg);
}

@media (max-width: 960px) {
  .span-4,
  .span-5,
  .span-7 {
    grid-column: span 6;
  }
  .span-12 {
    grid-column: span 12;
  }
}

@media (max-width: 720px) {
  .features {
    padding: 80px 16px 16px;
  }
  .bento {
    gap: 12px;
  }
  .span-4,
  .span-5,
  .span-7,
  .span-12 {
    grid-column: span 12;
  }
  .mobile {
    flex-direction: column;
    align-items: stretch;
  }
  .proto li {
    grid-template-columns: 56px 1fr auto;
  }
}

@media (prefers-reduced-motion: reduce) {
  .caret,
  .wire::after,
  .ring::before,
  .ring::after {
    animation: none;
  }
}
</style>
