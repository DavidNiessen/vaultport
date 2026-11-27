<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script setup lang="ts">
/** Illustration of the app: custom title bar with pinned tabs, a terminal and the side panel. */
import {
  Braces,
  FolderOpen,
  History,
  LayoutGrid,
  Palette,
  Plus,
  Search,
  Sparkles,
  SquareTerminal,
  Vault,
} from '@lucide/vue';

import TerminalDemo from './TerminalDemo.vue';

const snippets = [
  { name: 'Reload nginx', cmd: 'sudo systemctl reload {{service:nginx}}' },
  { name: 'Disk usage', cmd: 'df -h --total | tail -1' },
  { name: 'Tail app log', cmd: 'journalctl -fu {{unit}}' },
];
</script>

<template>
  <figure
    class="window"
    role="img"
    aria-label="Vaultport window: a terminal connected to prod-eu-1 through a jump host with post-quantum key exchange and a FIDO2 key, next to the snippets panel."
  >
    <div class="titlebar" aria-hidden="true">
      <div class="lights">
        <span class="light red" />
        <span class="light yellow" />
        <span class="light green" />
      </div>
      <div class="tabs">
        <span class="tab pinned"><Vault :size="14" /> <span class="label">Vault</span></span>
        <span class="tab pinned"><FolderOpen :size="14" /> <span class="label">SFTP</span></span>
        <span class="divider" />
        <span class="tab active"
          ><span class="dot ok" /><SquareTerminal :size="14" /> prod-eu-1</span
        >
        <span class="tab"><span class="dot info" /><SquareTerminal :size="14" /> db-replica</span>
        <span class="tab hide-sm"><LayoutGrid :size="14" /> web · 4 hosts</span>
        <span class="icon-btn"><Plus :size="14" /></span>
      </div>
      <span class="palette hide-sm"><Search :size="13" /> <span>Search</span><kbd>⌘K</kbd></span>
    </div>
    <div class="body" aria-hidden="true">
      <div class="term"><TerminalDemo /></div>
      <aside class="panel">
        <div class="panel-tabs">
          <span class="ptab active"><Braces :size="14" /></span>
          <span class="ptab"><History :size="14" /></span>
          <span class="ptab"><Palette :size="14" /></span>
          <span class="ptab"><Sparkles :size="14" /></span>
        </div>
        <p class="panel-title">Snippets</p>
        <div v-for="s in snippets" :key="s.name" class="snippet">
          <span class="snippet-name">{{ s.name }}</span>
          <code>{{ s.cmd }}</code>
        </div>
      </aside>
    </div>
  </figure>
</template>

<style scoped>
.window {
  /* The window always shows the dark app, so it keeps the dark accent in both site themes. */
  --sc-accent: #b3a6ff;
  --sc-accent-text: #b3a6ff;
  --sc-accent-rgb: 179 166 255;
  --sc-accent-fg: #0f0b1f;
  position: relative;
  margin: 0;
  overflow: hidden;
  border-radius: 18px;
  background: #15141c;
  box-shadow:
    0 0 0 1px rgb(255 255 255 / 0.09),
    0 1px 0 0 rgb(255 255 255 / 0.06) inset,
    0 50px 100px -30px rgb(0 0 0 / 0.7),
    0 30px 60px -30px rgb(79 70 229 / 0.45);
  color: #d8dcea;
  text-align: left;
}

.titlebar {
  display: flex;
  align-items: center;
  gap: 14px;
  height: 44px;
  padding: 0 12px 0 16px;
  border-bottom: 1px solid rgb(255 255 255 / 0.06);
  background: rgb(16 19 28 / 0.96);
  font-size: 12.5px;
}

.lights {
  display: flex;
  gap: 8px;
  flex-shrink: 0;
}

.light {
  width: 12px;
  height: 12px;
  border-radius: 50%;
  box-shadow: inset 0 0 0 0.5px rgb(0 0 0 / 0.25);
}
.red {
  background: #ff5f57;
}
.yellow {
  background: #febc2e;
}
.green {
  background: #28c840;
}

.tabs {
  display: flex;
  align-items: center;
  gap: 4px;
  min-width: 0;
  flex: 1;
  overflow: hidden;
}

.tab {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 28px;
  padding: 0 10px;
  border-radius: 999px;
  color: #9ba2b6;
  white-space: nowrap;
}

.tab.pinned .label {
  display: inline;
}

.tab.active {
  background: #1e2331;
  color: #e8eaf2;
  box-shadow: 0 0 0 1px rgb(255 255 255 / 0.06);
}

.divider {
  width: 1px;
  height: 16px;
  margin: 0 4px;
  background: rgb(255 255 255 / 0.1);
}

.dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
}
.dot.ok {
  background: #22c55e;
  box-shadow: 0 0 8px rgb(34 197 94 / 0.8);
}
.dot.info {
  background: #38bdf8;
}

.icon-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  border-radius: 8px;
  color: #6b7288;
}

.palette {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  height: 28px;
  padding: 0 6px 0 10px;
  border-radius: 999px;
  background: #161a25;
  box-shadow: 0 0 0 1px rgb(255 255 255 / 0.06);
  color: #6b7288;
  flex-shrink: 0;
}

.palette kbd {
  padding: 1px 6px;
  border-radius: 5px;
  background: #1e2331;
  font-family: var(--vp-font-family-mono);
  font-size: 11px;
}

.body {
  display: grid;
  grid-template-columns: 1fr 248px;
  height: 340px;
}

.term {
  min-width: 0;
  background:
    radial-gradient(120% 80% at 0% 0%, rgb(var(--sc-accent-rgb) / 0.08), transparent 60%), #15141c;
}

.panel {
  padding: 12px;
  border-left: 1px solid rgb(255 255 255 / 0.06);
  background: #10131c;
  font-size: 12px;
}

.panel-tabs {
  display: flex;
  gap: 4px;
  margin-bottom: 14px;
  padding: 3px;
  border-radius: 9px;
  background: #0d0c12;
}

.ptab {
  display: inline-flex;
  flex: 1;
  align-items: center;
  justify-content: center;
  height: 26px;
  border-radius: 999px;
  color: #6b7288;
}

.ptab.active {
  background: #1e2331;
  color: var(--sc-accent-text);
}

.panel-title {
  margin: 0 0 8px 2px;
  color: #6b7288;
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.06em;
  text-transform: uppercase;
}

.snippet {
  display: grid;
  gap: 4px;
  margin-bottom: 8px;
  padding: 10px;
  border-radius: 10px;
  background: #161a25;
  box-shadow: 0 0 0 1px rgb(255 255 255 / 0.05);
}

.snippet:first-of-type {
  box-shadow: 0 0 0 1px rgb(var(--sc-accent-rgb) / 0.45);
}

.snippet-name {
  color: #e8eaf2;
  font-weight: 500;
}

.snippet code {
  overflow: hidden;
  color: #7e86a3;
  font-family: var(--vp-font-family-mono);
  font-size: 10.5px;
  text-overflow: ellipsis;
  white-space: nowrap;
}

@media (max-width: 960px) {
  .body {
    grid-template-columns: 1fr;
  }
  .panel {
    display: none;
  }
}

@media (max-width: 640px) {
  .titlebar {
    gap: 10px;
    height: 40px;
    padding: 0 8px 0 12px;
    font-size: 11.5px;
  }
  .light {
    width: 10px;
    height: 10px;
  }
  .lights {
    gap: 6px;
  }
  .tab {
    padding: 0 7px;
  }
  .tab.pinned .label,
  .hide-sm {
    display: none;
  }
  .body {
    height: 236px;
  }
}
</style>
