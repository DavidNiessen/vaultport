<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script setup lang="ts">
/** Closing call to action: a headline lit by the pointer and a magnetic download button. */
import { ArrowRight, Download } from '@lucide/vue';
import { withBase } from 'vitepress';
import { computed, ref } from 'vue';

import { REPO_URL } from '../../../site';
import { OS_LABEL, usePlatform } from '../../composables/platform';
import GithubIcon from '../chrome/GithubIcon.vue';

const platform = usePlatform();
const label = computed(() =>
  platform.value?.os ? `Download for ${OS_LABEL[platform.value.os]}` : 'Download Vaultport',
);

const box = ref<HTMLElement | null>(null);
function move(e: PointerEvent) {
  const el = box.value;
  if (!el) return;
  const r = el.getBoundingClientRect();
  el.style.setProperty('--cx', `${e.clientX - r.left}px`);
  el.style.setProperty('--cy', `${e.clientY - r.top}px`);
}
</script>

<template>
  <section class="cta">
    <div ref="box" class="box container" @pointermove="move">
      <div class="glow" aria-hidden="true" />
      <h2 class="big" v-reveal>Take back<br /><span class="serif">your keys.</span></h2>
      <p class="lede" v-reveal="100">
        Free and open source on Windows, macOS, Linux, iOS and Android. No sign-up, ever.
      </p>
      <div class="actions" v-reveal="200">
        <a v-magnetic="0.3" :href="withBase('/download')" class="btn btn-primary btn-lg">
          <Download aria-hidden="true" /> {{ label }}
        </a>
        <a v-magnetic="0.3" :href="REPO_URL" target="_blank" rel="noreferrer" class="btn btn-lg">
          <GithubIcon /> View source <ArrowRight class="btn-arrow" aria-hidden="true" />
        </a>
      </div>
    </div>
  </section>
</template>

<style scoped>
.cta {
  padding: 120px 0 20px;
}
.box {
  --cx: 50%;
  --cy: 30%;
  position: relative;
  display: grid;
  justify-items: center;
  gap: 24px;
  padding-block: 100px;
  text-align: center;
  border-radius: var(--radius-xl);
  overflow: hidden;
  isolation: isolate;
}
.box::before {
  content: '';
  position: absolute;
  inset: 0;
  z-index: -2;
  border-radius: inherit;
  border: 1px solid var(--border);
  background:
    radial-gradient(
      600px circle at var(--cx) var(--cy),
      rgb(var(--glow-1) / 0.22),
      transparent 60%
    ),
    linear-gradient(180deg, var(--surface-2), transparent);
}
.glow {
  position: absolute;
  left: 50%;
  bottom: -60%;
  z-index: -1;
  width: 900px;
  height: 600px;
  margin-left: -450px;
  border-radius: 50%;
  background: conic-gradient(
    from 90deg,
    rgb(var(--glow-1)),
    rgb(var(--glow-2)),
    rgb(var(--glow-3)),
    rgb(var(--glow-1))
  );
  filter: blur(120px);
  opacity: 0.35;
  animation: spin 16s linear infinite;
}
@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}
.big {
  font-size: clamp(52px, 10vw, 150px);
  line-height: 0.92;
  font-weight: 650;
  letter-spacing: -0.06em;
  padding-bottom: 0.12em;
  background: radial-gradient(
    420px circle at var(--cx) var(--cy),
    var(--iris-300),
    var(--text) 60%
  );
  -webkit-background-clip: text;
  background-clip: text;
  color: transparent;
}
.big .serif {
  font-weight: 400;
  letter-spacing: -0.03em;
}
.lede {
  max-width: 520px;
}
.actions {
  display: flex;
  flex-wrap: wrap;
  justify-content: center;
  gap: 12px;
  margin-top: 8px;
}
</style>
