<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script setup lang="ts">
/** 404, told the way an SSH client would: the page's host key does not match. */
import { useRoute, withBase } from 'vitepress';
import { computed } from 'vue';

const route = useRoute();
const path = computed(() => route.path);
</script>

<template>
  <section class="nf">
    <div class="glitch" data-text="404" aria-hidden="true">404</div>
    <div class="term" role="img" aria-label="An SSH warning saying this page could not be found">
      <div class="term-bar"><span /><span /><span /></div>
      <pre class="mono"><span class="p">$</span> ssh vaultport.niessen.dev{{ path }}
<span class="w">@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@
@    WARNING: REMOTE PAGE IDENTIFICATION HAS VANISHED!   @
@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@</span>
No page was found at this path. It may have moved,
or the link may be mistyped.
<span class="p">$</span> <span class="cursor" /></pre>
    </div>
    <h1 class="h-section">This page <span class="serif grad-text">doesn't exist.</span></h1>
    <div class="actions">
      <a :href="withBase('/')" class="btn btn-primary">Take me home</a>
      <a :href="withBase('/guide/')" class="btn">Read the docs</a>
    </div>
  </section>
</template>

<style scoped>
.nf {
  display: grid;
  justify-items: center;
  gap: 28px;
  min-height: 100vh;
  padding: calc(var(--nav-h) + 60px) var(--gutter) 80px;
  text-align: center;
  background: radial-gradient(50% 40% at 50% 20%, rgb(var(--glow-1) / 0.18), transparent 70%);
}
.glitch {
  position: relative;
  font-size: clamp(110px, 22vw, 240px);
  font-weight: 800;
  letter-spacing: -0.07em;
  line-height: 0.85;
  background: var(--grad-text);
  -webkit-background-clip: text;
  background-clip: text;
  color: transparent;
}
.glitch::before,
.glitch::after {
  content: attr(data-text);
  position: absolute;
  inset: 0;
  background: inherit;
  -webkit-background-clip: text;
  background-clip: text;
}
.glitch::before {
  animation: glitch 2.6s infinite steps(1);
  text-shadow: -2px 0 rgb(var(--glow-3) / 0.7);
}
.glitch::after {
  animation: glitch 3.1s infinite steps(1) reverse;
  text-shadow: 2px 0 rgb(var(--glow-2) / 0.7);
}
@keyframes glitch {
  0%,
  86%,
  100% {
    clip-path: inset(0 0 100% 0);
    transform: none;
  }
  88% {
    clip-path: inset(20% 0 55% 0);
    transform: translate(-6px, 2px);
  }
  91% {
    clip-path: inset(60% 0 10% 0);
    transform: translate(5px, -2px);
  }
  94% {
    clip-path: inset(40% 0 35% 0);
    transform: translate(-3px, 1px);
  }
}
.term {
  width: min(620px, 100%);
  border-radius: var(--radius-lg);
  border: 1px solid var(--border);
  background: color-mix(in oklab, var(--code-bg) 85%, transparent);
  box-shadow: var(--shadow-lg);
  text-align: left;
  overflow: hidden;
}
.term-bar {
  display: flex;
  gap: 7px;
  padding: 12px 14px;
  border-bottom: 1px solid var(--border);
}
.term-bar span {
  width: 11px;
  height: 11px;
  border-radius: 50%;
  background: var(--surface-3);
}
pre {
  margin: 0;
  padding: 18px 20px;
  font-size: clamp(10px, 2.3vw, 13px);
  line-height: 1.7;
  color: var(--text-2);
  overflow-x: auto;
}
.p {
  color: var(--ok);
}
.w {
  color: var(--danger);
}
.cursor {
  display: inline-block;
  width: 0.6em;
  height: 1.1em;
  vertical-align: -0.2em;
  background: var(--accent);
  animation: blink 1s steps(1) infinite;
}
@keyframes blink {
  50% {
    opacity: 0;
  }
}
.actions {
  display: flex;
  flex-wrap: wrap;
  justify-content: center;
  gap: 12px;
}
</style>
