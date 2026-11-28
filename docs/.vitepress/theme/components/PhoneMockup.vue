<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script setup lang="ts">
/**
 * A phone drawn in CSS around a phone screenshot: bezel, side buttons, a status bar with the
 * camera island, and the home indicator. The screenshots have no system UI of their own, so the
 * status bar and the bottom strip take the colour of the app's first and last rows. Everything
 * scales with the element's width.
 */
import { withBase } from 'vitepress';

import { PHONE_SHOTS, type PhoneShot } from './phoneShots';

const props = withDefaults(defineProps<{ shot: PhoneShot; alt?: string; lazy?: boolean }>(), {
  alt: '',
  lazy: true,
});
const meta = PHONE_SHOTS[props.shot];
</script>

<template>
  <div class="phone" :style="{ '--top': meta.top, '--bottom': meta.bottom }">
    <div class="body">
      <span class="side side-power" aria-hidden="true" />
      <span class="side side-vol-up" aria-hidden="true" />
      <span class="side side-vol-down" aria-hidden="true" />
      <div class="screen">
        <div class="status" aria-hidden="true">
          <span class="time">9:41</span>
          <span class="island" />
          <span class="icons">
            <svg viewBox="0 0 18 12" class="signal">
              <rect x="0" y="8" width="3" height="4" rx="1" />
              <rect x="5" y="5.5" width="3" height="6.5" rx="1" />
              <rect x="10" y="3" width="3" height="9" rx="1" />
              <rect x="15" y="0" width="3" height="12" rx="1" />
            </svg>
            <svg viewBox="0 0 26 12" class="battery">
              <rect
                x="0.5"
                y="0.5"
                width="22"
                height="11"
                rx="3"
                fill="none"
                stroke="currentColor"
              />
              <rect x="2.5" y="2.5" width="16" height="7" rx="1.5" />
              <rect x="24" y="4" width="1.6" height="4" rx="0.8" />
            </svg>
          </span>
        </div>
        <img
          :src="withBase(`/screens/${shot}.webp`)"
          :alt="alt"
          width="780"
          height="1328"
          :loading="lazy ? 'lazy' : undefined"
        />
        <div class="home-strip" aria-hidden="true"><span class="indicator" /></div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.phone {
  /* Everything inside is sized relative to the phone's width. */
  container-type: inline-size;
}
.body {
  position: relative;
  padding: 3.4cqw;
  border-radius: 14cqw;
  background: linear-gradient(150deg, #3a3846, #16151c 40%, #24232d);
  box-shadow:
    inset 0 0 0 0.5cqw rgb(255 255 255 / 0.14),
    inset 0 0 0 1.2cqw #0b0a0f,
    0 40px 70px -24px rgb(0 0 0 / 0.75),
    0 12px 24px -12px rgb(0 0 0 / 0.5);
}
.screen {
  position: relative;
  overflow: hidden;
  border-radius: 10.8cqw;
  background: var(--bottom);
}
.screen img {
  display: block;
  width: 100%;
  height: auto;
}
.status {
  position: relative;
  display: flex;
  align-items: center;
  justify-content: space-between;
  height: 11cqw;
  padding: 0 8cqw;
  background: var(--top);
  color: #f2f1f7;
  font-family: var(--font-sans);
  font-size: 4.1cqw;
  font-weight: 600;
  letter-spacing: -0.01em;
}
.island {
  position: absolute;
  left: 50%;
  top: 2.6cqw;
  width: 26cqw;
  height: 7.4cqw;
  transform: translateX(-50%);
  border-radius: 99px;
  background: #000;
}
.icons {
  display: inline-flex;
  align-items: center;
  gap: 1.6cqw;
}
.icons svg {
  display: block;
  height: 3.1cqw;
  width: auto;
  fill: currentColor;
}
.home-strip {
  display: grid;
  place-items: center;
  height: 6cqw;
  background: var(--bottom);
}
.indicator {
  width: 34cqw;
  height: 1.3cqw;
  border-radius: 99px;
  background: rgb(255 255 255 / 0.75);
}
.side {
  position: absolute;
  width: 1cqw;
  border-radius: 1cqw;
  background: linear-gradient(90deg, #2a2933, #4a4856);
}
.side-power {
  right: -0.7cqw;
  top: 26%;
  height: 15%;
}
.side-vol-up,
.side-vol-down {
  left: -0.7cqw;
  height: 9%;
}
.side-vol-up {
  top: 20%;
}
.side-vol-down {
  top: 32%;
}
</style>
